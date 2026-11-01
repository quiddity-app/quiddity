//! Meta-properties: effective properties, inverses, queries and flags.
//!
//! Transitive closure and inverses are checked against reachability worked
//! out directly, queries against an evaluator over the written text, and
//! the rest by example.

use std::collections::BTreeSet;

use proptest::collection::vec;
use proptest::prelude::*;
use quiddity_core::schema::{Cardinality, ValueType};
use quiddity_core::syntax::{Query, Value, normalise_name, parse_query};
use quiddity_core::{BlockId, Flag, Graph, Index, Result};

/// Makes an entity whose properties block is `props`, if it isn't empty.
fn entity(graph: &Graph, name: &str, props: &str) -> Result<BlockId> {
    let id = graph.create_block(None, None, name)?;
    if !props.is_empty() {
        graph.create_block(Some(id), None, props)?;
    }
    Ok(id)
}

fn q(src: &str) -> Query {
    parse_query(src).unwrap()
}

/// The entities a query finds, by name.
fn found(index: &Index, query: &str) -> Vec<String> {
    let mut names: Vec<_> = index
        .query(&q(query))
        .into_iter()
        .map(|id| index.entity(id).unwrap().name.clone())
        .collect();
    names.sort();
    names
}

fn targets(index: &Index, id: BlockId, key: &str) -> BTreeSet<BlockId> {
    index
        .effective_values(id, key)
        .iter()
        .filter_map(|i| i.target)
        .collect()
}

const N: usize = 5;

proptest! {
    /// A transitive property reaches what following it step by step does,
    /// cycles included; its inverse is the same relation the other way
    /// round, and a query sees both.
    #[test]
    fn transitive_closure(
        edges in vec(vec(0..N, 0..3), N),
        transitive in any::<bool>(),
    ) {
        let graph = Graph::new();
        let meta = if transitive { "##transitive\n" } else { "" };
        entity(&graph, "parent", &format!("type:: property\n{meta}##inverse(children)"))?;
        let ids: Vec<BlockId> = (0..N)
            .map(|i| {
                let list: Vec<_> = edges[i].iter().map(|j| format!("e{j}")).collect();
                let props = if list.is_empty() {
                    String::new()
                } else {
                    format!("parent:: [{}]", list.join(", "))
                };
                entity(&graph, &format!("e{i}"), &props)
            })
            .collect::<Result<_>>()?;
        let index = Index::build(&graph)?;

        let reach = |i: usize| -> BTreeSet<usize> {
            let mut seen = BTreeSet::new();
            let mut stack: Vec<usize> = edges[i].clone();
            while let Some(j) = stack.pop() {
                if seen.insert(j) && transitive {
                    stack.extend(&edges[j]);
                }
            }
            seen
        };
        for i in 0..N {
            let expected: BTreeSet<BlockId> = reach(i).iter().map(|&j| ids[j]).collect();
            prop_assert_eq!(targets(&index, ids[i], "parent"), expected.clone(), "e{}", i);

            let parents_of: BTreeSet<BlockId> =
                (0..N).filter(|&k| reach(k).contains(&i)).map(|k| ids[k]).collect();
            let all: BTreeSet<_> = index.inverse("children", ids[i], false).into_iter().collect();
            prop_assert_eq!(&all, &parents_of, "children of e{}", i);
            let direct: BTreeSet<_> = index.inverse("Children", ids[i], true).into_iter().collect();
            let written: BTreeSet<_> =
                (0..N).filter(|&k| edges[k].contains(&i)).map(|k| ids[k]).collect();
            prop_assert_eq!(direct, written, "direct children of e{}", i);

            let by_query: BTreeSet<_> =
                index.query(&q(&format!("#parent(e{i})"))).into_iter().collect();
            prop_assert_eq!(by_query, parents_of, "#parent(e{})", i);
            let by_inverse: BTreeSet<_> =
                index.query(&q(&format!("#children(e{i})"))).into_iter().collect();
            prop_assert_eq!(by_inverse, expected, "#children(e{})", i);

            let cycle = index.flags(ids[i]).iter().any(|f| matches!(f, Flag::Cycle { .. }));
            prop_assert_eq!(cycle, transitive && reach(i).contains(&i), "e{}", i);
        }
    }
}

// Small pools, so that names and values collide. Values are text, so a
// query's meaning can be worked out from what is written alone.
const NAMES: [&str; 3] = ["Ann", "bob", "Cat"];
const KEYS: [&str; 2] = ["kind", "Size"];
const VALUES: [&str; 3] = ["a", "A ", "b"];

/// An entity: a name, and properties with text values, lists or none.
type Written = (usize, Vec<(usize, Option<Vec<usize>>)>);

fn written() -> impl Strategy<Value = Written> {
    (
        0..NAMES.len(),
        vec(
            (
                0..KEYS.len(),
                proptest::option::of(vec(0..VALUES.len(), 1..3)),
            ),
            0..4,
        ),
    )
}

fn query() -> impl Strategy<Value = Query> {
    let leaf = prop_oneof![
        (0..NAMES.len()).prop_map(|i| Query::Name(NAMES[i].to_uppercase())),
        (0..VALUES.len()).prop_map(|i| Query::Name(VALUES[i].to_owned())),
        (0..KEYS.len()).prop_map(|k| Query::Has {
            key: KEYS[k].to_lowercase(),
            value: None
        }),
    ];
    leaf.prop_recursive(3, 12, 2, |inner| {
        let pair = (inner.clone(), inner.clone()).prop_map(|(a, b)| (Box::new(a), Box::new(b)));
        prop_oneof![
            (0..KEYS.len(), inner.clone()).prop_map(|(k, v)| Query::Has {
                key: KEYS[k].to_owned(),
                value: Some(Box::new(v))
            }),
            inner.prop_map(|a| Query::Not(Box::new(a))),
            pair.clone().prop_map(|(a, b)| Query::And(a, b)),
            pair.clone().prop_map(|(a, b)| Query::Diff(a, b)),
            pair.prop_map(|(a, b)| Query::Or(a, b)),
        ]
    })
}

/// Whether a written entity matches a query, from the text alone.
fn oracle(entity: &Written, query: &Query) -> bool {
    let values = |key: &str| -> Vec<Option<String>> {
        entity
            .1
            .iter()
            .filter(|(k, _)| normalise_name(KEYS[*k]) == normalise_name(key))
            .flat_map(|(_, v)| match v {
                None => vec![None],
                Some(list) => list
                    .iter()
                    .map(|&i| Some(normalise_name(VALUES[i])))
                    .collect(),
            })
            .collect()
    };
    match query {
        Query::Name(n) => normalise_name(NAMES[entity.0]) == normalise_name(n),
        Query::Has { key, value: None } => !values(key).is_empty(),
        Query::Has {
            key,
            value: Some(v),
        } => values(key).iter().any(|item| item_oracle(item, v)),
        Query::Not(a) => !oracle(entity, a),
        Query::And(a, b) => oracle(entity, a) && oracle(entity, b),
        Query::Diff(a, b) => oracle(entity, a) && !oracle(entity, b),
        Query::Or(a, b) => oracle(entity, a) || oracle(entity, b),
        Query::Link(_) => unreachable!(),
    }
}

fn item_oracle(item: &Option<String>, query: &Query) -> bool {
    match query {
        Query::Name(n) => item.as_deref() == Some(normalise_name(n).as_str()),
        Query::Has { .. } => false,
        Query::Not(a) => !item_oracle(item, a),
        Query::And(a, b) => item_oracle(item, a) && item_oracle(item, b),
        Query::Diff(a, b) => item_oracle(item, a) && !item_oracle(item, b),
        Query::Or(a, b) => item_oracle(item, a) || item_oracle(item, b),
        Query::Link(_) => unreachable!(),
    }
}

proptest! {
    /// Queries find what they say, in names, keys and values compared as
    /// names are, with `&`, `|` and `~` per entity and, inside `#key(…)`,
    /// per value.
    #[test]
    fn queries_find_what_they_say(entities in vec(written(), 1..5), query in query()) {
        let graph = Graph::new();
        let mut ids = Vec::new();
        for (name, props) in &entities {
            let lines: Vec<String> = props
                .iter()
                .map(|(k, v)| match v {
                    None => format!("#{}", KEYS[*k]),
                    Some(list) => {
                        let items: Vec<_> = list.iter().map(|&i| VALUES[i].trim()).collect();
                        format!("{}:: [{}]", KEYS[*k], items.join(", "))
                    }
                })
                .collect();
            ids.push(entity(&graph, NAMES[*name], &lines.join("\n"))?);
        }
        let index = Index::build(&graph)?;
        let found: BTreeSet<_> = index.query(&query).into_iter().collect();
        let expected: BTreeSet<_> = entities
            .iter()
            .zip(&ids)
            .filter(|(e, _)| oracle(e, &query))
            .map(|(_, &id)| id)
            .collect();
        prop_assert_eq!(found, expected, "{:?}", query);
    }
}

#[test]
fn transitive_types() -> Result<()> {
    let graph = Graph::new();
    entity(&graph, "Oak", "type:: tree")?;
    entity(&graph, "tree", "type:: plant")?;
    entity(&graph, "plant", "")?;
    let index = Index::build(&graph)?;
    assert_eq!(found(&index, "#type(plant)"), ["tree"]);

    entity(&graph, "type", "type:: property\n##transitive")?;
    let index = Index::build(&graph)?;
    assert_eq!(found(&index, "#type(plant)"), ["Oak", "tree"]);
    assert_eq!(
        index.schema("type").value_type,
        ValueType::Ref,
        "the built-in type is still a reference"
    );
    Ok(())
}

#[test]
fn inheritance() -> Result<()> {
    let graph = Graph::new();
    entity(
        &graph,
        "supertype",
        "type:: property\n##transitive\n##inherits\n##inverse(subtypes)",
    )?;
    let living = entity(&graph, "living thing", "#alive\nkingdom:: any")?;
    let plant = entity(
        &graph,
        "plant",
        "supertype:: living thing\ncolour:: green\nalias:: flora\ntitle:: Plants",
    )?;
    let tree = entity(&graph, "tree", "supertype:: plant\ncolour:: brown")?;
    let index = Index::build(&graph)?;

    let props = index.effective_properties(tree);
    let keys: Vec<_> = props.keys().map(String::as_str).collect();
    assert_eq!(keys, ["alive", "colour", "kingdom", "supertype"]);
    let colours: Vec<_> = index
        .effective_values(tree, "colour")
        .into_iter()
        .map(|i| (i.value, i.from))
        .collect();
    assert_eq!(
        colours,
        [(Some(Value::Text("brown".into())), None)],
        "its own values come first"
    );
    let alive = index.effective_values(tree, "alive");
    assert_eq!(alive.len(), 1);
    assert_eq!(alive[0].from, Some(living), "from where it is written");
    assert_eq!(targets(&index, tree, "supertype"), [plant, living].into());
    assert_eq!(found(&index, "#alive"), ["living thing", "plant", "tree"]);
    assert_eq!(found(&index, "#subtypes(tree)"), ["living thing", "plant"]);
    assert_eq!(index.inverse("subtypes", living, true), [plant]);
    Ok(())
}

/// Inheriting doesn't carry the inheriting property itself unless it is
/// transitive, nor what identifies an entity.
#[test]
fn what_is_not_inherited() -> Result<()> {
    let graph = Graph::new();
    entity(&graph, "kind of", "type:: property\n##inherits")?;
    entity(
        &graph,
        "plant",
        "kind of:: living thing\nid:: p1\ndate:: 2026-11-01\nalias:: flora\ntitle:: Plants\nleaves:: yes",
    )?;
    let tree = entity(&graph, "tree", "kind of:: plant")?;
    entity(&graph, "living thing", "")?;
    let index = Index::build(&graph)?;
    let keys: Vec<_> = index.effective_properties(tree).into_keys().collect();
    assert_eq!(keys, ["kind of", "leaves"]);
    Ok(())
}

/// An entity is a direct inverse if any property with the label refers to
/// it as written, even if another reaches it only transitively.
#[test]
fn inverse_labels_shared() -> Result<()> {
    let graph = Graph::new();
    entity(
        &graph,
        "parent",
        "type:: property\n##transitive\n##inverse(children)",
    )?;
    entity(&graph, "guardian", "type:: property\n##inverse(children)")?;
    let grandma = entity(&graph, "Grandma", "")?;
    let mum = entity(&graph, "Mum", "parent:: Grandma")?;
    let kid = entity(&graph, "Kid", "parent:: Mum\nguardian:: Grandma")?;
    let index = Index::build(&graph)?;
    assert_eq!(index.inverse("children", grandma, true), [mum, kid]);
    assert_eq!(index.inverse("children", mum, true), [kid]);
    Ok(())
}

#[test]
fn symmetric() -> Result<()> {
    let graph = Graph::new();
    entity(&graph, "sibling", "type:: property\n##symmetric")?;
    let ann = entity(&graph, "Ann", "sibling:: Bob")?;
    let bob = entity(&graph, "Bob", "")?;
    let index = Index::build(&graph)?;
    assert_eq!(targets(&index, bob, "sibling"), [ann].into());
    assert_eq!(found(&index, "#sibling(Ann)"), ["Bob"]);
    assert_eq!(found(&index, "#sibling(Bob)"), ["Ann"]);
    Ok(())
}

/// Values match by text, by the name or alias of the entity they refer to,
/// or by what a nested query finds of it.
#[test]
fn values_in_queries() -> Result<()> {
    let graph = Graph::new();
    entity(&graph, "Scotland", "type:: country\nalias:: Alba")?;
    entity(&graph, "Lothian", "type:: region")?;
    entity(&graph, "Edinburgh", "in:: [[Scotland]]\nrating:: 5")?;
    entity(&graph, "Leith", "in:: [[Lothian]]\nrating:: 4")?;
    entity(&graph, "Nowhere", "in:: [[Atlantis]]\nrating:: 3")?;
    entity(&graph, "Flag", "colour:: Scotland")?;
    entity(&graph, "Twin", "rating:: 1")?;
    entity(&graph, "Twin", "rating:: 2")?;
    let index = Index::build(&graph)?;
    assert_eq!(
        found(&index, "#colour(#type(country))"),
        [] as [&str; 0],
        "bare text refers to nothing in a text property"
    );
    assert_eq!(found(&index, "#colour(Scotland)"), ["Flag"]);
    assert_eq!(
        found(&index, "[[Twin]]"),
        ["Twin", "Twin"],
        "an ambiguous link finds all"
    );
    assert_eq!(found(&index, "#rating((4 | 5) ~ 4)"), ["Edinburgh"]);
    assert_eq!(
        found(&index, "#rating(4 & 5)"),
        [] as [&str; 0],
        "one value that is both"
    );
    assert_eq!(found(&index, "#rating(5)"), ["Edinburgh"]);
    assert_eq!(found(&index, "#rating(4 | 5)"), ["Edinburgh", "Leith"]);
    assert_eq!(
        found(&index, "#rating(~3)"),
        ["Edinburgh", "Leith", "Twin", "Twin"]
    );
    assert_eq!(found(&index, "#in(alba)"), ["Edinburgh"]);
    assert_eq!(
        found(&index, "#in([[Scotland#type(country)]])"),
        ["Edinburgh"]
    );
    assert_eq!(found(&index, "#in(#type(country))"), ["Edinburgh"]);
    assert_eq!(
        found(&index, "#in(#type(country) | #type(region))"),
        ["Edinburgh", "Leith"]
    );
    assert_eq!(
        found(&index, "#in(Atlantis)"),
        ["Nowhere"],
        "by text, with nothing to refer to"
    );
    assert_eq!(found(&index, "[[Alba]] | Leith"), ["Leith", "Scotland"]);
    Ok(())
}

#[test]
fn flags() -> Result<()> {
    let graph = Graph::new();
    entity(
        &graph,
        "born in",
        "type:: property\n##values(#type(place))\n##applies-to(#type(person))\n##cardinality(1)\n##inverse(births)",
    )?;
    entity(&graph, "rating", "type:: property\n##value-type(number)")?;
    entity(&graph, "Paris", "type:: place")?;
    entity(&graph, "Bob", "type:: dog")?;
    let ann = entity(
        &graph,
        "Ann",
        "type:: person\nborn in:: [Paris, Bob]\nrating:: lots",
    )?;
    let cat = entity(&graph, "Cat", "type:: person")?;
    let dog = entity(&graph, "Rex", "type:: dog\nborn in:: Paris")?;
    let stray = entity(&graph, "Stray", "##transitive")?;
    let births = entity(&graph, "births", "type:: property")?;
    let odd = entity(
        &graph,
        "odd",
        "type:: property\n##colour(red)\n##cardinality(lots)",
    )?;
    let index = Index::build(&graph)?;

    let bob = index.named("Bob")[0];
    let ann_flags = index.flags(ann);
    assert!(
        matches!(&ann_flags[0], Flag::Mismatch { key, .. } if key == "rating"),
        "{ann_flags:?}"
    );
    assert_eq!(
        ann_flags[1..],
        [
            Flag::NotInValues {
                key: "born in".into(),
                target: bob
            },
            Flag::Cardinality {
                key: "born in".into(),
                count: 2,
                allowed: Cardinality {
                    min: 1,
                    max: Some(1)
                }
            },
        ]
    );
    assert_eq!(
        index.flags(cat),
        [Flag::Cardinality {
            key: "born in".into(),
            count: 0,
            allowed: Cardinality {
                min: 1,
                max: Some(1)
            }
        }],
        "a property it applies to and doesn't have"
    );
    assert_eq!(
        index.flags(dog),
        [Flag::NotApplicable {
            key: "born in".into()
        }]
    );
    assert_eq!(index.flags(stray), [Flag::MetaOutsideProperty]);
    assert_eq!(
        index.flags(births),
        [Flag::InverseClash {
            label: "births".into(),
            property: "born in".into()
        }]
    );
    let odd_flags = index.flags(odd);
    assert!(
        matches!(&odd_flags[..], [Flag::UnknownMeta { key }, Flag::BadMeta { key: c, .. }] if key == "colour" && c == "cardinality"),
        "{odd_flags:?}"
    );
    Ok(())
}

/// A property entity's meta-properties override a built-in schema.
#[test]
fn builtins_can_be_overridden() -> Result<()> {
    let graph = Graph::new();
    entity(
        &graph,
        "date",
        "type:: property\n##value-type(text)\n##cardinality(*)",
    )?;
    let index = Index::build(&graph)?;
    let date = index.schema("Date");
    assert_eq!(
        (date.value_type, date.cardinality),
        (ValueType::Text, Cardinality::ANY)
    );
    assert_eq!(index.schema("favourite").value_type, ValueType::Boolean);
    Ok(())
}
