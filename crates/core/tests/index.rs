//! Link resolution and backlinks against a brute-force oracle that reads the
//! graph directly.

mod common;

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::Index as Pick;
use quiddity_core::syntax::{Link, Qualifier, Value, format_link, normalise_name};
use quiddity_core::{Backlink, BlockId, Graph, Index, Resolution};

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

/// Text for a note: some words and links, to the entities' names and
/// qualified, so that a block links to the same entity more than once, and
/// some links are ambiguous and some are not.
fn note_text() -> impl Strategy<Value = String> {
    let part = prop_oneof![
        2 => Just("words ".to_owned()),
        2 => (0..NAMES.len()).prop_map(|i| format!("[[{}]] ", NAMES[i])),
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
        .create_block(Some(jane), None, "quotable\nid:: abc")
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
    // An `id` property, which `((id))` refers to, either side.
    assert!(!index.update_text(referred, "quotable\nid:: abc", "quotable [[Jane]]"));
    assert!(!index.update_text(note, "see [[Jane]]", "see\nid:: abc"));
    // A block the index has never seen.
    let new = graph.create_block(Some(jane), None, "new").unwrap();
    assert!(!index.update_text(new, "new", "newer [[Jane]]"));

    assert_eq!(index.backlinks(jane), before, "declining changes nothing");
    assert_eq!(index.resolve_block_ref("abc"), Some(referred));
}
