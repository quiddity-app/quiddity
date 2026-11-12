//! Link resolution and backlinks against a brute-force oracle that reads the
//! graph directly.

mod common;

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::Index as Pick;
use quiddity_core::syntax::{Link, Qualifier, Value, format_link, format_tag, normalise_name};
use quiddity_core::{Backlink, BlockId, Graph, Index, Resolution, canonical_value};

// Small pools, so that names, keys and values collide often. Names and
// values differ in case and spacing to exercise normalisation, and `alias`
// gives entities second names.
const NAMES: [&str; 4] = ["Jane", "jane  ", "Bob", "C# notes"];
const KEYS: [&str; 3] = ["kind", "role", "alias"];
const VALUES: [&str; 4] = ["a", "A ", "b", "Bob"];

fn value() -> impl Strategy<Value = Value> {
    let text = (0..VALUES.len()).prop_map(|i| Value::Text(VALUES[i].to_owned()));
    let link = (0..VALUES.len()).prop_map(|i| {
        Value::Link(Link {
            name: VALUES[i].to_owned(),
            qualifiers: Vec::new(),
        })
    });
    prop_oneof![
        2 => text.clone(),
        1 => link,
        1 => vec(text, 0..3).prop_map(Value::List),
    ]
}

type Entity = (usize, Vec<(usize, Option<Value>)>);

fn entity() -> impl Strategy<Value = Entity> {
    (
        0..NAMES.len(),
        vec(
            (0..KEYS.len(), proptest::option::weighted(0.9, value())),
            0..4,
        ),
    )
}

#[derive(Debug, Clone)]
enum Query {
    /// Any name and qualifiers: mostly ambiguous or missing.
    Random(usize, Vec<(usize, Option<Value>)>),
    /// Some of an existing entity's own properties, so it should match.
    Derived(Pick, Vec<bool>),
}

fn query() -> impl Strategy<Value = Query> {
    prop_oneof![
        (
            0..NAMES.len(),
            vec((0..KEYS.len(), proptest::option::of(value())), 0..3)
        )
            .prop_map(|(n, q)| Query::Random(n, q)),
        (any::<Pick>(), vec(any::<bool>(), 4)).prop_map(|(e, mask)| Query::Derived(e, mask)),
    ]
}

/// Names and values compare ignoring case and repeated spaces; a link
/// value compares by its name.
fn canonical(v: &Value) -> Option<String> {
    match v {
        Value::Text(s) => Some(normalise_name(s)),
        Value::Link(l) => Some(normalise_name(&l.name)),
        _ => None,
    }
}

fn value_matches(have: &Value, want: &Value) -> bool {
    match (have, want) {
        (Value::List(items), Value::List(wants)) => {
            items.len() == wants.len() && items.iter().zip(wants).all(|(h, w)| value_matches(h, w))
        }
        (Value::List(items), _) => items.iter().any(|i| value_matches(i, want)),
        _ => canonical(have).is_some() && canonical(have) == canonical(want),
    }
}

/// Every entity the link should resolve to, by checking each one directly.
fn oracle(graph: &Graph, link: &Link) -> Vec<BlockId> {
    let want_name = normalise_name(&link.name);
    let mut matches: Vec<_> = graph
        .roots()
        .into_iter()
        .filter(|&e| {
            let props = graph.entity_properties(e).unwrap();
            let aliases = props
                .iter()
                .filter(|p| normalise_name(&p.key) == "alias")
                .filter_map(|p| p.value.as_ref())
                .flat_map(|v| match v {
                    Value::List(items) => items.clone(),
                    v => vec![v.clone()],
                })
                .filter_map(|v| canonical(&v));
            let named = std::iter::once(normalise_name(&graph.entity_name(e).unwrap()))
                .chain(aliases)
                .any(|n| n == want_name);
            named
                && link.qualifiers.iter().all(|q| {
                    props.iter().any(|p| {
                        normalise_name(&p.key) == normalise_name(&q.key)
                            && match (&q.value, &p.value) {
                                (None, _) => true,
                                (Some(want), Some(have)) => value_matches(have, want),
                                (Some(_), None) => false,
                            }
                    })
                })
        })
        .collect();
    matches.sort();
    matches
}

proptest! {
    #[test]
    fn resolution_matches_oracle(entities in vec(entity(), 1..6), query in query()) {
        let graph = Graph::new();
        let ids: Vec<BlockId> = entities
            .iter()
            .map(|(name, props)| {
                let props: Vec<_> = props.iter().map(|(k, v)| (KEYS[*k], v.clone())).collect();
                graph.create_entity(NAMES[*name], &props).unwrap()
            })
            .collect();

        let link = match &query {
            Query::Random(name, qualifiers) => Link {
                name: NAMES[*name].to_uppercase(),
                qualifiers: qualifiers
                    .iter()
                    .map(|(k, v)| Qualifier { key: KEYS[*k].to_owned(), value: v.clone() })
                    .collect(),
            },
            Query::Derived(pick, mask) => {
                let i = pick.index(entities.len());
                let (name, props) = &entities[i];
                Link {
                    name: format!(" {} ", NAMES[*name]),
                    qualifiers: props
                        .iter()
                        .zip(mask)
                        .filter(|(_, keep)| **keep)
                        .map(|((k, v), _)| Qualifier { key: KEYS[*k].to_uppercase(), value: v.clone() })
                        .collect(),
                }
            }
        };

        // The link is written into a note, so it is also a backlink.
        let notes = graph.create_entity("Notes", &[]).unwrap();
        let note = graph.create_block(Some(notes), None, &format!("see {}", format_link(&link))).unwrap();
        graph.commit();

        let index = Index::build(&graph).unwrap();
        let expected = oracle(&graph, &link);
        if let Query::Derived(pick, _) = &query {
            prop_assert!(expected.contains(&ids[pick.index(entities.len())]), "oracle misses its source entity");
        }
        let resolution = match expected.as_slice() {
            [] => Resolution::Missing,
            [one] => Resolution::Unique(*one),
            many => Resolution::Ambiguous(many.to_vec()),
        };
        prop_assert_eq!(index.resolve(&link), resolution);

        for &id in &ids {
            let backlink = Backlink { source: note, ambiguous: expected.len() > 1 };
            let linked = index.backlinks(id).contains(&backlink);
            prop_assert_eq!(linked, expected.contains(&id), "backlink to {}", id);
        }
    }
}

/// Text for a note: some words, links and tags, to the entities' names and
/// qualified, so that a block links to the same entity more than once, and
/// some links are ambiguous and some are not.
fn note_text() -> impl Strategy<Value = String> {
    let part = prop_oneof![
        2 => Just("words ".to_owned()),
        2 => (0..NAMES.len()).prop_map(|i| format!("[[{}]] ", NAMES[i])),
        // Names nobody has made, spelt differently, linked and as a tag.
        1 => Just("[[Nowhere]] ".to_owned()),
        1 => Just("[[ no  where ]] ".to_owned()),
        1 => Just("see #nowhere ".to_owned()),
        // A tag is a link too. With words before it, so the block isn't all
        // tags, which would make it a properties block.
        1 => (0..NAMES.len()).prop_map(|i| format!("see {} ", format_tag(NAMES[i]))),
        1 => (0..NAMES.len()).prop_map(|i| {
            let link = Link {
                name: NAMES[i].to_owned(),
                qualifiers: vec![Qualifier {
                    key: "kind".to_owned(),
                    value: Some(Value::Text("a".to_owned())),
                }],
            };
            format!("{} ", format_link(&link))
        }),
    ];
    vec(part, 0..4).prop_map(|parts| parts.concat())
}

proptest! {
    /// Updating the index for an edited block gives exactly what building it
    /// again would, backlinks in the same order.
    #[test]
    fn updating_a_block_is_building_again(
        entities in vec(entity(), 1..5),
        notes in vec(note_text(), 1..6),
        edits in vec((any::<Pick>(), note_text()), 1..8),
    ) {
        let graph = Graph::new();
        let ids: Vec<BlockId> = entities
            .iter()
            .map(|(name, props)| {
                let props: Vec<_> = props.iter().map(|(k, v)| (KEYS[*k], v.clone())).collect();
                graph.create_entity(NAMES[*name], &props).unwrap()
            })
            .collect();
        let page = graph.create_entity("Notes", &[]).unwrap();
        // Spread over two entities, so that blocks' order isn't their creation order.
        let blocks: Vec<BlockId> = notes
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let parent = if i % 2 == 0 { page } else { ids[0] };
                graph.create_block(Some(parent), None, text).unwrap()
            })
            .collect();
        graph.commit();

        let mut index = Index::build(&graph).unwrap();
        for (pick, text) in &edits {
            let block = blocks[pick.index(blocks.len())];
            let old = graph.text(block).unwrap();
            graph.set_text(block, text).unwrap();
            graph.commit();
            prop_assert!(index.update_text(block, &old, text), "an ordinary block can be updated");

            let fresh = Index::build(&graph).unwrap();
            for &entity in ids.iter().chain([&page]) {
                prop_assert_eq!(index.backlinks(entity), fresh.backlinks(entity), "backlinks of {}", entity);
            }
            prop_assert_eq!(
                index.mentions().collect::<Vec<_>>(),
                fresh.mentions().collect::<Vec<_>>(),
                "mentions"
            );
        }
    }
}

/// What the index can't update for it says so, and is left as it was.
#[test]
fn update_declines_what_changes_more_than_backlinks() {
    let graph = Graph::new();
    let jane = graph
        .create_entity("Jane", &[("kind", Some(Value::Text("a".into())))])
        .unwrap();
    let props = graph.children(jane)[0];
    let note = graph
        .create_block(Some(jane), None, "see [[Jane]]")
        .unwrap();
    let referred = graph
        .create_block(Some(jane), None, "quotable\n##id(abc)")
        .unwrap();
    graph.commit();
    let mut index = Index::build(&graph).unwrap();
    let before = index.backlinks(jane).to_vec();
    assert_eq!(before.len(), 1);

    // A root's text is a name.
    assert!(!index.update_text(jane, "Jane", "Janet [[Jane]]"));
    // A properties block, either side of the edit.
    assert!(!index.update_text(props, "kind:: a", "kind:: a [[Jane]]"));
    assert!(!index.update_text(note, "see [[Jane]]", "kind:: b"));
    assert!(!index.update_text(note, "kind:: b", "see [[Jane]]"));
    // What makes a properties block is its first line, whatever follows it.
    assert!(!index.update_text(note, "see [[Jane]]", "kind:: b\nsee [[Jane]]"));
    assert!(!index.update_text(note, "see [[Jane]]", "#tag"));
    assert!(index.update_text(note, "see [[Jane]]", "see [[Jane]] #tag"));
    // An `##id` line, which `((id))` refers to, either side.
    assert!(!index.update_text(referred, "quotable\n##id(abc)", "quotable [[Jane]]"));
    assert!(!index.update_text(note, "see [[Jane]]", "see\n##id(abc)"));
    // A line with no ID refers to nothing, so it doesn't stop an update.
    assert!(index.update_text(note, "see [[Jane]]", "see [[Jane]]\n##id"));
    // A block the index has never seen.
    let new = graph.create_block(Some(jane), None, "new").unwrap();
    assert!(!index.update_text(new, "new", "newer [[Jane]]"));

    assert_eq!(index.backlinks(jane), before, "declining changes nothing");
    assert_eq!(index.resolve_block_ref("abc"), Some(referred));
}

/// Prose in a properties block is read as nothing: it adds no property and
/// no link, and is said to be stray so that it can be flagged.
#[test]
fn prose_in_a_properties_block_is_stray() {
    use quiddity_core::Flag;
    let graph = Graph::new();
    let jane = graph.create_block(None, None, "Jane").unwrap();
    graph
        .create_block(
            Some(jane),
            None,
            "kind:: a\nMet [[Bob]] at #summit\n#friend\n\nrole:: b",
        )
        .unwrap();
    let bob = graph.create_block(None, None, "Bob").unwrap();
    graph.create_block(Some(bob), None, "kind:: a").unwrap();
    graph.commit();
    let index = Index::build(&graph).unwrap();
    let info = index.entity(jane).unwrap();
    let keys: Vec<_> = info.properties.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, ["kind", "friend", "role"]);
    assert_eq!(info.stray, ["Met [[Bob]] at #summit"]);
    assert!(
        index.backlinks(bob).is_empty(),
        "a stray line's link isn't read"
    );
    assert_eq!(
        index.flags(jane),
        [Flag::StrayLine {
            text: "Met [[Bob]] at #summit".into()
        }]
    );
    assert!(index.entity(bob).unwrap().stray.is_empty());
    assert_eq!(index.flags(bob), []);
}

/// A first block that starts with prose is an ordinary block, whatever else
/// it holds: its entity has no properties.
#[test]
fn a_block_that_starts_with_prose_is_not_a_properties_block() {
    let graph = Graph::new();
    let jane = graph.create_block(None, None, "Jane").unwrap();
    graph
        .create_block(Some(jane), None, "see below\nkind:: a")
        .unwrap();
    graph.commit();
    let index = Index::build(&graph).unwrap();
    let info = index.entity(jane).unwrap();
    assert!(info.properties.is_empty() && info.stray.is_empty());
}

/// A tag is a link to the entity it names, wherever it is written: in prose,
/// as `#a` in a properties block, or as a value of `##tag`. Each is one
/// backlink from its block, however it is written there.
#[test]
fn tags_are_links() {
    let graph = Graph::new();
    let person = graph.create_block(None, None, "person").unwrap();
    let org = graph.create_block(None, None, "org").unwrap();
    let team = graph.create_block(None, None, "Team").unwrap();
    let jane = graph.create_block(None, None, "Jane").unwrap();
    let props = graph
        .create_block(Some(jane), None, "#person\n##tag:: [org, [[Team]], person]")
        .unwrap();
    let note = graph
        .create_block(Some(jane), None, "met a #person and a #[[Team]] here")
        .unwrap();
    graph.commit();
    let index = Index::build(&graph).unwrap();
    let from =
        |entity| -> Vec<BlockId> { index.backlinks(entity).iter().map(|b| b.source).collect() };
    // Twice from the properties block: as `#person` and as a `##tag` value.
    assert_eq!(from(person), [props, props, note]);
    assert_eq!(from(org), [props]);
    assert_eq!(
        from(team),
        [props, note],
        "a link value is one link, not two"
    );
    assert!(from(jane).is_empty());
}

/// `#a` and `##tag:: a` are the same tag: the entity has it either way, and
/// a link qualifier can ask for it.
#[test]
fn both_forms_of_a_tag_are_the_entity_s_tag() {
    let graph = Graph::new();
    let short = graph.create_block(None, None, "Mercury").unwrap();
    graph.create_block(Some(short), None, "#planet").unwrap();
    let long = graph.create_block(None, None, "Mercury").unwrap();
    graph
        .create_block(Some(long), None, "##tag:: [planet, hot]")
        .unwrap();
    let other = graph.create_block(None, None, "Mercury").unwrap();
    graph
        .create_block(Some(other), None, "tags:: [planet]")
        .unwrap();
    graph.commit();
    let index = Index::build(&graph).unwrap();
    let keys = |e| -> Vec<String> {
        index
            .entity(e)
            .unwrap()
            .properties
            .iter()
            .map(|p| p.key.clone())
            .collect()
    };
    assert_eq!(keys(short), ["planet"]);
    assert_eq!(keys(long), ["planet", "hot"]);
    assert_eq!(
        keys(other),
        ["tags"],
        "`tags::` is an ordinary property now"
    );
    let ask = |tag: &str| {
        let link = Link {
            name: "Mercury".into(),
            qualifiers: vec![Qualifier {
                key: tag.into(),
                value: None,
            }],
        };
        match index.resolve(&link) {
            Resolution::Unique(id) => vec![id],
            Resolution::Ambiguous(ids) => ids,
            Resolution::Missing => vec![],
        }
    };
    assert_eq!(ask("planet"), [short, long]);
    assert_eq!(ask("hot"), [long]);
    assert!(ask("cold").is_empty());
}

/// An entity's aliases are the items of every `alias` property, a single name
/// or a list, and nothing else.
#[test]
fn aliases_are_the_items_of_every_alias_property() {
    let graph = Graph::new();
    let root = graph.create_block(None, None, "Mercury").unwrap();
    graph
        .create_block(
            Some(root),
            None,
            "alias:: Quicksilver\nalias:: [Hermes, Messenger]\nkind:: planet",
        )
        .unwrap();
    let index = Index::build(&graph).unwrap();
    let info = index.entities().next().unwrap();
    let aliases: Vec<String> = info.aliases().into_iter().map(canonical_value).collect();
    assert_eq!(aliases, ["quicksilver", "hermes", "messenger"]);
}

/// A name that is linked to and that no entity has is a mention: wherever the
/// link is written, a link or a tag, and spelt as it likes. Qualifiers, an
/// entity of that name or one reached by an alias make it none.
#[test]
fn a_name_nobody_has_is_a_mention() {
    let graph = Graph::new();
    let jane = graph.create_block(None, None, "Jane").unwrap();
    graph.create_block(Some(jane), None, "alias:: JD").unwrap();
    let notes = graph.create_block(None, None, "Notes").unwrap();
    let first = graph
        .create_block(Some(notes), None, "see [[Nowhere]] and [[Jane]] and [[JD]]")
        .unwrap();
    let second = graph
        .create_block(
            Some(notes),
            None,
            "again [[ nowhere ]], #Nowhere and #[[also  gone]]",
        )
        .unwrap();
    let qualified = graph
        .create_block(Some(notes), None, "[[Elsewhere#kind(a)]]")
        .unwrap();
    graph.commit();
    let index = Index::build(&graph).unwrap();

    let names: Vec<_> = index.mentions().map(|(k, _)| k).collect();
    assert_eq!(names, ["also gone", "nowhere"]);
    let from = |name: &str| -> Vec<(BlockId, String)> {
        index
            .mention(name)
            .iter()
            .map(|m| (m.source, m.name.clone()))
            .collect()
    };
    assert_eq!(
        from("NOWHERE"),
        [
            (first, "Nowhere".to_owned()),
            (second, "nowhere".to_owned()),
            (second, "Nowhere".to_owned()),
        ],
        "in document order, each as it was written"
    );
    assert_eq!(from("also gone").len(), 1);
    assert!(from("Jane").is_empty() && from("JD").is_empty());
    assert!(
        from("Elsewhere").is_empty(),
        "a link with qualifiers makes no mention"
    );
    let _ = qualified;
}

/// Making the entity ends the mention, and a name with no links to it isn't one.
#[test]
fn a_mention_ends_when_the_entity_is_made() {
    let graph = Graph::new();
    let notes = graph.create_block(None, None, "Notes").unwrap();
    graph
        .create_block(Some(notes), None, "see [[Nowhere]]")
        .unwrap();
    graph.commit();
    assert_eq!(Index::build(&graph).unwrap().mentions().count(), 1);
    graph.create_block(None, None, "nowhere").unwrap();
    graph.commit();
    assert_eq!(Index::build(&graph).unwrap().mentions().count(), 0);
}
