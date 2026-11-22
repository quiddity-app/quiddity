//! Contacts from a graph, and edits made to them elsewhere coming back.
//!
//! What goes in comes out is a property over arbitrary text; the rest is
//! examples of the ways an edit can meet a graph that has moved on.

use proptest::prelude::*;
use quiddity_contacts::{Contact, Entry, apply, contact_of, contacts, forget};
use quiddity_core::contact::ContactField as F;
use quiddity_core::{BlockId, Graph, Index, Result};

/// Property entities for every field, some with labels, and one with a
/// space in its name.
const PROPERTIES: &[(&str, &str)] = &[
    ("mobile", "phone:mobile"),
    ("landline", "phone:home"),
    ("phone", "phone"),
    ("email", "email"),
    ("work email", "email:work"),
    ("address", "address"),
    ("employer", "organisation"),
    ("job title", "job-title"),
    ("nickname", "nickname"),
    ("website", "website"),
    ("born", "birthday"),
    ("about", "note"),
];

fn graph() -> Result<Graph> {
    let graph = Graph::new();
    for (name, field) in PROPERTIES {
        let entity = graph.create_block(None, None, name)?;
        graph.create_block(Some(entity), None, &format!("##contact({field})"))?;
    }
    Ok(graph)
}

fn person(graph: &Graph, name: &str, properties: &str) -> Result<BlockId> {
    let id = graph.create_block(None, None, name)?;
    if !properties.is_empty() {
        graph.create_block(Some(id), None, properties)?;
    }
    Ok(id)
}

fn entry(field: F, label: Option<&str>, value: &str) -> Entry {
    Entry {
        field,
        label: label.map(str::to_owned),
        value: value.to_owned(),
    }
}

fn contact(name: &str, entries: Vec<Entry>) -> Contact {
    Contact {
        id: String::new(),
        name: name.to_owned(),
        entries,
    }
}

fn index(graph: &Graph) -> Result<Index> {
    Index::build(graph)
}

fn sorted(mut entries: Vec<Entry>) -> Vec<Entry> {
    entries.sort();
    entries
}

/// The text of the entity's properties block.
fn properties_text(graph: &Graph, id: BlockId) -> Result<String> {
    Ok(graph
        .properties_block(id)?
        .map(|b| graph.text(b))
        .transpose()?
        .unwrap_or_default())
}

#[test]
fn an_entity_with_a_mapped_value_is_a_contact() -> Result<()> {
    let graph = graph()?;
    let jane = person(
        &graph,
        "Jane Smith",
        "mobile:: 07700 900123\nemail:: jane@example.com\nwork email:: [jane@work.example, jsmith@work.example]\ntype:: person",
    )?;
    person(&graph, "Plain", "type:: person\nmobile::")?;
    person(&graph, "Nothing", "")?;
    let index = index(&graph)?;

    let all = contacts(&index);
    assert_eq!(all.len(), 1, "{all:?}");
    assert_eq!(all[0].id, jane.to_string());
    assert_eq!(all[0].name, "Jane Smith");
    assert_eq!(
        all[0].entries,
        [
            entry(F::Phone, Some("mobile"), "07700 900123"),
            entry(F::Email, None, "jane@example.com"),
            entry(F::Email, Some("work"), "jane@work.example"),
            entry(F::Email, Some("work"), "jsmith@work.example"),
        ],
        "in the order written, a list as its items, and no entry for a property that has no value"
    );
    assert_eq!(contact_of(&index, jane), Some(all[0].clone()));
    Ok(())
}

#[test]
fn links_and_days_read_as_text() -> Result<()> {
    let graph = graph()?;
    person(
        &graph,
        "Jane",
        "address:: [[Edinburgh]]\nborn:: [[2026-10-25]]\nabout:: ((abc))",
    )?;
    let index = index(&graph)?;
    assert_eq!(
        contacts(&index)[0].entries,
        [
            entry(F::Address, None, "Edinburgh"),
            entry(F::Birthday, None, "2026-10-25"),
        ],
        "a link is its name, and a block reference is nothing"
    );
    Ok(())
}

#[test]
fn an_edit_is_made_to_what_the_entity_is_now() -> Result<()> {
    let graph = graph()?;
    let jane = person(
        &graph,
        "Jane",
        "type:: person\nmobile:: 111\nemail:: old@example.com\nnickname:: J",
    )?;
    let base = contact_of(&index(&graph)?, jane).unwrap();

    // The graph moves on after the contact was handed over.
    graph.set_text(
        graph.properties_block(jane)?.unwrap(),
        "type:: person\nmobile:: 111\nemail:: old@example.com\nnickname:: J\nwebsite:: https://jane.example",
    )?;

    // The copy loses the email, gains a phone number and a new name.
    let mut desired = base.clone();
    desired.name = "Jane Smith".to_owned();
    desired.entries.remove(1);
    desired.entries.push(entry(F::Phone, Some("home"), "222"));
    let applied = apply(&graph, &index(&graph)?, Some(jane), Some(&base), &desired)?;

    assert_eq!(applied.id, Some(jane));
    assert!(applied.changed);
    assert_eq!(applied.dropped, []);
    let index = index(&graph)?;
    let now = contact_of(&index, jane).unwrap();
    assert_eq!(now.name, "Jane Smith");
    assert_eq!(
        sorted(now.entries),
        sorted(vec![
            entry(F::Phone, Some("mobile"), "111"),
            entry(F::Phone, Some("home"), "222"),
            entry(F::Nickname, None, "J"),
            entry(F::Website, None, "https://jane.example"),
        ]),
        "the website, which the copy never saw, is still there"
    );
    assert!(
        properties_text(&graph, jane)?.contains("type:: person"),
        "{}",
        properties_text(&graph, jane)?
    );
    assert_eq!(
        index.named("Jane").len(),
        1,
        "the old name is an alias, so links to it still find her"
    );
    Ok(())
}

#[test]
fn a_rename_waits_for_a_name_that_hasnt_changed() -> Result<()> {
    let graph = graph()?;
    let jane = person(&graph, "Jane", "mobile:: 111")?;
    let base = contact_of(&index(&graph)?, jane).unwrap();
    graph.rename_entity(jane, "Janet")?;
    let mut desired = base.clone();
    desired.name = "Jane Smith".to_owned();
    apply(&graph, &index(&graph)?, Some(jane), Some(&base), &desired)?;
    assert_eq!(graph.entity_name(jane)?, "Janet", "the graph's rename wins");
    Ok(())
}

#[test]
fn nothing_changed_is_not_a_change() -> Result<()> {
    let graph = graph()?;
    let jane = person(
        &graph,
        "Jane",
        "mobile:: 111\nemail:: [a@x.example, b@x.example]",
    )?;
    let base = contact_of(&index(&graph)?, jane).unwrap();
    let before = graph.snapshot()?;
    let applied = apply(&graph, &index(&graph)?, Some(jane), Some(&base), &base)?;
    assert!(!applied.changed);
    assert_eq!(graph.snapshot()?.len(), before.len());
    assert_eq!(
        properties_text(&graph, jane)?,
        "mobile:: 111\nemail:: [a@x.example, b@x.example]"
    );
    Ok(())
}

#[test]
fn a_list_keeps_what_isnt_removed_and_a_line_goes_when_empty() -> Result<()> {
    let graph = graph()?;
    let jane = person(
        &graph,
        "Jane",
        "email:: [a@x.example, b@x.example, c@x.example]\nmobile:: 111\nnickname:: J",
    )?;
    let base = contact_of(&index(&graph)?, jane).unwrap();
    let mut desired = base.clone();
    desired
        .entries
        .retain(|e| e.value != "b@x.example" && e.value != "111");
    apply(&graph, &index(&graph)?, Some(jane), Some(&base), &desired)?;
    assert_eq!(
        properties_text(&graph, jane)?,
        "email:: [a@x.example, c@x.example]\nnickname:: J"
    );
    Ok(())
}

#[test]
fn a_new_entry_goes_in_the_nearest_property() -> Result<()> {
    let graph = graph()?;
    let jane = person(&graph, "Jane", "nickname:: J")?;
    let base = contact_of(&index(&graph)?, jane).unwrap();
    let mut desired = base.clone();
    desired.entries.extend([
        entry(F::Phone, Some("work"), "1"), // no work phone: the plain one
        entry(F::Phone, Some("Mobile"), "2"), // labels compare as names do
        entry(F::Email, Some("home"), "h@x.example"), // no home email: the plain one
    ]);
    let applied = apply(&graph, &index(&graph)?, Some(jane), Some(&base), &desired)?;
    assert_eq!(applied.dropped, []);
    assert_eq!(
        properties_text(&graph, jane)?,
        "nickname:: J\nphone:: 1\nmobile:: 2\nemail:: h@x.example"
    );
    Ok(())
}

#[test]
fn an_entry_with_nowhere_to_go_is_dropped() -> Result<()> {
    let graph = Graph::new();
    let phone = graph.create_block(None, None, "phone")?;
    graph.create_block(Some(phone), None, "##contact(phone)")?;
    let jane = person(&graph, "Jane", "phone:: 1")?;
    let base = contact_of(&index(&graph)?, jane).unwrap();
    let mut desired = base.clone();
    desired.entries.push(entry(F::Email, None, "j@x.example"));
    let applied = apply(&graph, &index(&graph)?, Some(jane), Some(&base), &desired)?;
    assert_eq!(applied.dropped, [entry(F::Email, None, "j@x.example")]);
    assert!(!applied.changed);
    Ok(())
}

#[test]
fn a_contact_with_no_entity_makes_one() -> Result<()> {
    let graph = graph()?;
    let desired = contact(
        "Bob",
        vec![
            entry(F::Phone, Some("mobile"), "07700 900456"),
            entry(F::Email, Some("home"), "bob@example.com"),
            entry(F::Phone, Some("mobile"), "second"),
        ],
    );
    let applied = apply(&graph, &index(&graph)?, None, None, &desired)?;
    let id = applied.id.expect("made");
    assert!(applied.changed);
    assert_eq!(graph.entity_name(id)?, "Bob");
    assert_eq!(
        properties_text(&graph, id)?,
        "mobile:: 07700 900456\nemail:: bob@example.com\nmobile:: second"
    );
    let index = index(&graph)?;
    assert_eq!(contacts(&index).len(), 1);
    Ok(())
}

#[test]
fn a_nameless_contact_is_named_for_its_first_value() -> Result<()> {
    let graph = graph()?;
    let desired = contact("  ", vec![entry(F::Phone, None, "0131 496 0000")]);
    let id = apply(&graph, &index(&graph)?, None, None, &desired)?
        .id
        .unwrap();
    assert_eq!(graph.entity_name(id)?, "0131 496 0000");
    Ok(())
}

#[test]
fn nothing_to_make_it_from_makes_nothing() -> Result<()> {
    let graph = graph()?;
    let roots = graph.roots().len();
    for desired in [
        contact("Bob", vec![]),
        contact("", vec![]),
        contact("Bob", vec![entry(F::Phone, None, "")]),
    ] {
        let applied = apply(&graph, &index(&graph)?, None, None, &desired)?;
        assert_eq!((applied.id, applied.changed), (None, false), "{desired:?}");
    }
    let unmapped = Graph::new();
    let desired = contact("Bob", vec![entry(F::Phone, None, "1")]);
    let applied = apply(&unmapped, &index(&unmapped)?, None, None, &desired)?;
    assert_eq!(applied.id, None);
    assert_eq!(applied.dropped, desired.entries);
    assert_eq!(graph.roots().len(), roots);
    assert!(unmapped.roots().is_empty());
    Ok(())
}

#[test]
fn an_entity_that_is_gone_is_made_again_only_if_the_copy_was_edited() -> Result<()> {
    let graph = graph()?;
    let jane = person(&graph, "Jane", "mobile:: 111")?;
    let base = contact_of(&index(&graph)?, jane).unwrap();
    graph.delete_block(jane)?;

    let untouched = apply(&graph, &index(&graph)?, Some(jane), Some(&base), &base)?;
    assert_eq!((untouched.id, untouched.changed), (None, false));

    let mut edited = base.clone();
    edited.entries.push(entry(F::Phone, None, "222"));
    let made = apply(&graph, &index(&graph)?, Some(jane), Some(&base), &edited)?;
    let id = made.id.expect("made again");
    assert_ne!(id, jane);
    assert_eq!(contact_of(&index(&graph)?, id).unwrap().entries.len(), 2);
    Ok(())
}

#[test]
fn forgetting_keeps_the_entity() -> Result<()> {
    let graph = graph()?;
    let jane = person(
        &graph,
        "Jane",
        "type:: person\nmobile:: 111\nemail:: [a@x.example, b@x.example]\nalias:: J",
    )?;
    assert!(forget(&graph, &index(&graph)?, jane)?);
    assert_eq!(properties_text(&graph, jane)?, "type:: person\nalias:: J");
    assert_eq!(contacts(&index(&graph)?), []);
    assert!(
        !forget(&graph, &index(&graph)?, jane)?,
        "nothing left to remove"
    );
    Ok(())
}

#[test]
fn a_rename_alone_is_a_change() -> Result<()> {
    let graph = graph()?;
    let jane = person(&graph, "Jane", "mobile:: 111")?;
    let base = contact_of(&index(&graph)?, jane).unwrap();
    let mut desired = base.clone();
    desired.name = "Jane Smith".to_owned();
    let applied = apply(&graph, &index(&graph)?, Some(jane), Some(&base), &desired)?;
    assert!(applied.changed, "the name is all that changed");
    assert_eq!(graph.entity_name(jane)?, "Jane Smith");
    Ok(())
}

#[test]
fn an_entity_with_no_properties_gets_a_block_that_starts_with_a_property() -> Result<()> {
    let graph = graph()?;
    let jane = person(&graph, "Jane", "")?;
    let base = contact("Jane", vec![]);
    let desired = contact("Jane", vec![entry(F::Phone, Some("mobile"), "111")]);
    apply(&graph, &index(&graph)?, Some(jane), Some(&base), &desired)?;
    assert_eq!(
        properties_text(&graph, jane)?,
        "mobile:: 111",
        "no blank line before the first"
    );
    Ok(())
}

#[test]
fn a_property_whose_name_cant_be_a_key_is_not_written_to() -> Result<()> {
    // Parentheses can't be in the key of a property line, so a value written
    // under this name would not read back as one.
    let graph = Graph::new();
    let phone = graph.create_block(None, None, "phone (work)")?;
    graph.create_block(Some(phone), None, "##contact(phone)")?;
    let jane = person(&graph, "Jane", "phone (work):: 1")?;
    let base = contact_of(&index(&graph)?, jane);
    assert_eq!(
        base, None,
        "it isn't even read as one: the line isn't a property"
    );
    let desired = contact("Jane", vec![entry(F::Phone, None, "2")]);
    let applied = apply(&graph, &index(&graph)?, Some(jane), None, &desired)?;
    assert_eq!(applied.dropped, desired.entries);
    assert!(!applied.changed);
    Ok(())
}

fn text() -> impl Strategy<Value = String> {
    // The characters the syntax treats specially, and a newline.
    "[a-z0-9+ ,:\\[\\]()#\"\\\\\n-]{1,14}"
        .prop_filter("something to show", |s| !s.trim().is_empty())
}

/// An entry in a field and label that a property fills.
fn mapped_entry() -> impl Strategy<Value = Entry> {
    (
        proptest::sample::select(vec![
            (F::Phone, Some("mobile")),
            (F::Phone, Some("home")),
            (F::Phone, None),
            (F::Email, None),
            (F::Email, Some("work")),
            (F::Address, None),
            (F::Organisation, None),
            (F::JobTitle, None),
            (F::Nickname, None),
            (F::Website, None),
            (F::Birthday, None),
            (F::Note, None),
        ]),
        text(),
    )
        .prop_map(|((field, label), value)| Entry {
            field,
            label: label.map(str::to_owned),
            value: value.trim().to_owned(),
        })
}

proptest! {
    /// Whatever a contact holds, an entity made from it is a contact that
    /// holds the same, however its values are written.
    #[test]
    fn a_made_contact_reads_back(entries in proptest::collection::vec(mapped_entry(), 1..6)) {
        let graph = graph()?;
        let applied = apply(&graph, &index(&graph)?, None, None, &contact("Someone", entries.clone()))?;
        let id = applied.id.expect("made");
        let now = contact_of(&index(&graph)?, id).expect("a contact");
        prop_assert_eq!(sorted(now.entries), sorted(entries));
        prop_assert_eq!(applied.dropped, []);
    }

    /// Any edit to a contact whose entity hasn't moved on gives an entity
    /// that is the edited contact, whatever was added, removed or repeated.
    #[test]
    fn an_edit_gives_the_edited_contact(
        before in proptest::collection::vec(mapped_entry(), 1..6),
        after in proptest::collection::vec(mapped_entry(), 1..6),
        keep in proptest::collection::vec(any::<bool>(), 6),
    ) {
        let graph = graph()?;
        let id = apply(&graph, &index(&graph)?, None, None, &contact("Someone", before))?
            .id
            .expect("made");
        let base = contact_of(&index(&graph)?, id).expect("a contact");

        // Some of what was there stays, and some new is added.
        let mut desired = base.clone();
        let mut flags = keep.iter().cycle();
        desired.entries.retain(|_| *flags.next().unwrap());
        desired.entries.extend(after);
        prop_assume!(!desired.entries.is_empty());

        let applied = apply(&graph, &index(&graph)?, Some(id), Some(&base), &desired)?;
        prop_assert_eq!(applied.dropped, []);
        let now = contact_of(&index(&graph)?, id).expect("a contact");
        prop_assert_eq!(sorted(now.entries), sorted(desired.entries));
        prop_assert_eq!(now.name, "Someone");
    }
}

#[test]
fn an_entry_the_entity_gained_itself_is_not_added_again() -> Result<()> {
    let graph = graph()?;
    let jane = person(&graph, "Jane", "mobile:: 111")?;
    let base = contact_of(&index(&graph)?, jane).unwrap();
    // Both sides add the same email, and the copy a second, equal phone number.
    graph.set_text(
        graph.properties_block(jane)?.unwrap(),
        "mobile:: 111\nemail:: j@x.example",
    )?;
    let mut desired = base.clone();
    desired.entries.push(entry(F::Email, None, "j@x.example"));
    desired.entries.push(entry(F::Phone, Some("mobile"), "111"));
    apply(&graph, &index(&graph)?, Some(jane), Some(&base), &desired)?;
    assert_eq!(
        properties_text(&graph, jane)?,
        "mobile:: 111\nemail:: j@x.example\nmobile:: 111"
    );
    Ok(())
}
