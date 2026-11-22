//! Keeping a phone's copy of the contacts and the graph in step, against a
//! store that behaves as the phone does: it remembers what it was handed,
//! lets the user edit its copy, and says which rows were touched.

use std::collections::BTreeMap;
use std::convert::Infallible;

use proptest::prelude::*;
use quiddity_contacts::{Contact, Edited, Entry, Held, Report, Store, contacts, reconcile};
use quiddity_core::contact::ContactField as F;
use quiddity_core::{BlockId, Graph, Index, Result};

#[derive(Debug, Clone)]
struct Row {
    /// As handed over.
    held: Contact,
    /// What the user sees and edits.
    shown: Contact,
    dirty: bool,
    deleted: bool,
}

#[derive(Debug, Default)]
struct Phone {
    rows: BTreeMap<String, Row>,
    next: usize,
    /// Rows put and removed, for the tests that count them.
    log: Vec<String>,
}

impl Store for Phone {
    type Error = Infallible;

    fn edited(&mut self) -> std::result::Result<Vec<Edited>, Infallible> {
        Ok(self
            .rows
            .iter()
            .filter(|(_, r)| r.dirty)
            .map(|(name, r)| Edited {
                row: name.clone(),
                id: (!r.held.id.is_empty()).then(|| r.held.id.clone()),
                base: (!r.held.id.is_empty()).then(|| r.held.clone()),
                contact: r.shown.clone(),
                deleted: r.deleted,
            })
            .collect())
    }

    fn held(&mut self) -> std::result::Result<Vec<Held>, Infallible> {
        Ok(self
            .rows
            .iter()
            .map(|(row, r)| Held {
                row: row.clone(),
                contact: r.held.clone(),
            })
            .collect())
    }

    fn put(
        &mut self,
        row: Option<&str>,
        contact: &Contact,
    ) -> std::result::Result<String, Infallible> {
        let name = row.map_or_else(
            || {
                self.next += 1;
                format!("row{}", self.next)
            },
            str::to_owned,
        );
        self.log.push(format!("put {name}"));
        self.rows.insert(
            name.clone(),
            Row {
                held: contact.clone(),
                shown: contact.clone(),
                dirty: false,
                deleted: false,
            },
        );
        Ok(name)
    }

    fn remove(&mut self, row: &str) -> std::result::Result<(), Infallible> {
        self.log.push(format!("remove {row}"));
        self.rows.remove(row);
        Ok(())
    }
}

impl Phone {
    fn edit(&mut self, row: &str, f: impl FnOnce(&mut Contact)) {
        let r = self.rows.get_mut(row).expect("a row");
        f(&mut r.shown);
        r.dirty = true;
    }

    fn delete(&mut self, row: &str) {
        let r = self.rows.get_mut(row).expect("a row");
        r.deleted = true;
        r.dirty = true;
    }

    /// A contact made on the phone.
    fn create(&mut self, contact: Contact) -> String {
        self.next += 1;
        let name = format!("made{}", self.next);
        self.rows.insert(
            name.clone(),
            Row {
                held: Contact {
                    id: String::new(),
                    name: String::new(),
                    entries: vec![],
                },
                shown: contact,
                dirty: true,
                deleted: false,
            },
        );
        name
    }

    fn row_of(&self, name: &str) -> String {
        self.rows
            .iter()
            .find(|(_, r)| r.shown.name == name)
            .map(|(row, _)| row.clone())
            .unwrap_or_else(|| panic!("no row for {name}: {self:?}"))
    }

    fn shown(&self) -> Vec<Contact> {
        let mut all: Vec<_> = self.rows.values().map(|r| r.shown.clone()).collect();
        all.sort_by(|a, b| a.id.cmp(&b.id));
        all
    }
}

fn graph() -> Result<Graph> {
    let graph = Graph::new();
    for (name, field) in [
        ("mobile", "phone:mobile"),
        ("phone", "phone"),
        ("email", "email"),
        ("nickname", "nickname"),
    ] {
        let entity = graph.create_block(None, None, name)?;
        graph.create_block(Some(entity), None, &format!("##contact({field})"))?;
    }
    Ok(graph)
}

fn person(graph: &Graph, name: &str, properties: &str) -> Result<BlockId> {
    let id = graph.create_block(None, None, name)?;
    graph.create_block(Some(id), None, properties)?;
    Ok(id)
}

fn entry(field: F, label: Option<&str>, value: &str) -> Entry {
    Entry {
        field,
        label: label.map(str::to_owned),
        value: value.to_owned(),
    }
}

fn sync(graph: &Graph, phone: &mut Phone) -> Report {
    reconcile(graph, phone).expect("the fake store doesn't fail")
}

fn current(graph: &Graph) -> Vec<Contact> {
    let mut all = contacts(&Index::build(graph).unwrap());
    all.sort_by(|a, b| a.id.cmp(&b.id));
    all
}

fn properties(graph: &Graph, id: BlockId) -> String {
    graph
        .text(graph.properties_block(id).unwrap().unwrap())
        .unwrap()
}

#[test]
fn contacts_are_handed_over_once() -> Result<()> {
    let graph = graph()?;
    person(&graph, "Jane", "mobile:: 111")?;
    person(&graph, "Bob", "email:: b@x.example")?;
    person(&graph, "Nobody", "nickname::")?;
    let mut phone = Phone::default();

    let first = sync(&graph, &mut phone);
    assert_eq!((first.put, first.removed, first.changed), (2, 0, false));
    assert_eq!(phone.shown(), current(&graph));

    phone.log.clear();
    let second = sync(&graph, &mut phone);
    assert_eq!(second, Report::default());
    assert_eq!(phone.log, Vec::<String>::new(), "nothing touched");
    Ok(())
}

#[test]
fn the_graphs_changes_reach_the_phone() -> Result<()> {
    let graph = graph()?;
    let jane = person(&graph, "Jane", "mobile:: 111")?;
    let bob = person(&graph, "Bob", "email:: b@x.example")?;
    let mut phone = Phone::default();
    sync(&graph, &mut phone);

    graph.set_text(graph.properties_block(jane)?.unwrap(), "mobile:: 222")?;
    graph.rename_entity(bob, "Robert")?;
    graph.set_text(graph.properties_block(bob)?.unwrap(), "email::")?;
    person(&graph, "Cat", "phone:: 333")?;
    let report = sync(&graph, &mut phone);

    assert_eq!((report.put, report.removed), (2, 1));
    assert_eq!(phone.shown(), current(&graph));
    assert_eq!(phone.rows.len(), 2, "Bob is no contact now");
    Ok(())
}

#[test]
fn an_edit_on_the_phone_reaches_the_graph() -> Result<()> {
    let graph = graph()?;
    let jane = person(&graph, "Jane", "mobile:: 111\nnickname:: J")?;
    let mut phone = Phone::default();
    sync(&graph, &mut phone);

    let row = phone.row_of("Jane");
    phone.edit(&row, |c| {
        c.name = "Jane Smith".to_owned();
        c.entries.remove(0);
        c.entries.push(entry(F::Email, None, "jane@x.example"));
    });
    let report = sync(&graph, &mut phone);

    assert!(report.changed);
    assert_eq!(
        (report.made, report.put, report.removed),
        (0, 1, 0),
        "the row was edited, so it was put back as the graph has it, once"
    );
    assert_eq!(graph.entity_name(jane)?, "Jane Smith");
    assert_eq!(
        properties(&graph, jane),
        "nickname:: J\nemail:: jane@x.example\nalias:: Jane",
        "a rename keeps the old name"
    );
    assert!(!phone.rows[&row].dirty, "put back as the graph has it");
    assert_eq!(phone.shown(), current(&graph));
    Ok(())
}

#[test]
fn both_sides_changing_keeps_both() -> Result<()> {
    let graph = graph()?;
    let jane = person(&graph, "Jane", "mobile:: 111")?;
    let mut phone = Phone::default();
    sync(&graph, &mut phone);

    // The graph gains a nickname while the phone gains an email.
    graph.set_text(
        graph.properties_block(jane)?.unwrap(),
        "mobile:: 111\nnickname:: J",
    )?;
    let row = phone.row_of("Jane");
    phone.edit(&row, |c| {
        c.entries.push(entry(F::Email, None, "j@x.example"))
    });
    sync(&graph, &mut phone);

    let jane_now = current(&graph).remove(0);
    assert_eq!(jane_now.entries.len(), 3, "{jane_now:?}");
    assert_eq!(phone.shown(), current(&graph));
    Ok(())
}

#[test]
fn a_contact_made_on_the_phone_is_made_in_the_graph() -> Result<()> {
    let graph = graph()?;
    let mut phone = Phone::default();
    let row = phone.create(Contact {
        id: String::new(),
        name: "Dee".to_owned(),
        entries: vec![
            entry(F::Phone, Some("mobile"), "444"),
            entry(F::Website, None, "https://dee.example"),
        ],
    });
    let report = sync(&graph, &mut phone);

    assert_eq!((report.made, report.changed), (1, true));
    assert_eq!(
        report.dropped.len(),
        1,
        "nothing in the graph fills website"
    );
    assert_eq!(report.dropped[0].0, "Dee");
    let made = current(&graph);
    assert_eq!(made.len(), 1);
    assert_eq!(
        phone.rows[&row].held.id, made[0].id,
        "the row is the entity's now"
    );
    assert_eq!(phone.shown(), made);
    assert_eq!(phone.rows.len(), 1, "no second row for it");
    Ok(())
}

#[test]
fn a_contact_that_cant_be_kept_is_removed_and_reported() -> Result<()> {
    let graph = graph()?;
    let mut phone = Phone::default();
    phone.create(Contact {
        id: String::new(),
        name: "Dee".to_owned(),
        entries: vec![entry(F::Website, None, "https://dee.example")],
    });
    let report = sync(&graph, &mut phone);
    assert_eq!(report.dropped.len(), 1);
    assert_eq!((report.made, report.removed), (0, 1));
    assert!(phone.rows.is_empty());
    assert!(graph.roots().len() == 4, "no entity made");
    Ok(())
}

#[test]
fn a_contact_edited_down_to_nothing_is_no_contact() -> Result<()> {
    let graph = graph()?;
    let jane = person(&graph, "Jane", "type:: person\nmobile:: 111")?;
    let mut phone = Phone::default();
    sync(&graph, &mut phone);

    let row = phone.row_of("Jane");
    phone.edit(&row, |c| c.entries.clear());
    let report = sync(&graph, &mut phone);

    assert_eq!((report.removed, report.made, report.put), (1, 0, 0));
    assert!(report.changed);
    assert!(phone.rows.is_empty());
    assert_eq!(properties(&graph, jane), "type:: person");
    Ok(())
}

#[test]
fn deleting_on_the_phone_keeps_the_page() -> Result<()> {
    let graph = graph()?;
    let jane = person(&graph, "Jane", "type:: person\nmobile:: 111")?;
    let mut phone = Phone::default();
    sync(&graph, &mut phone);

    let row = phone.row_of("Jane");
    phone.delete(&row);
    let report = sync(&graph, &mut phone);

    assert_eq!(report.removed, 1);
    assert!(report.changed, "the values came off the entity");
    assert!(phone.rows.is_empty());
    assert_eq!(properties(&graph, jane), "type:: person");
    assert!(graph.contains(jane));

    phone.log.clear();
    sync(&graph, &mut phone);
    assert!(phone.rows.is_empty(), "she doesn't come back");
    Ok(())
}

#[test]
fn an_edit_to_a_contact_deleted_in_the_graph_makes_it_again() -> Result<()> {
    let graph = graph()?;
    let jane = person(&graph, "Jane", "mobile:: 111")?;
    let mut phone = Phone::default();
    sync(&graph, &mut phone);
    graph.delete_block(jane)?;
    let row = phone.row_of("Jane");
    phone.edit(&row, |c| {
        c.entries.push(entry(F::Email, None, "j@x.example"))
    });
    let report = sync(&graph, &mut phone);
    assert_eq!(report.made, 1, "a new entity, not the one that was edited");
    let now = current(&graph);
    assert_eq!(now.len(), 1);
    assert_eq!(now[0].entries.len(), 2);
    assert_eq!(phone.shown(), now);
    Ok(())
}

#[test]
fn an_untouched_contact_deleted_in_the_graph_goes_from_the_phone() -> Result<()> {
    let graph = graph()?;
    let jane = person(&graph, "Jane", "mobile:: 111")?;
    let mut phone = Phone::default();
    sync(&graph, &mut phone);
    graph.delete_block(jane)?;
    let report = sync(&graph, &mut phone);
    assert_eq!(report.removed, 1);
    assert!(phone.rows.is_empty());
    Ok(())
}

#[derive(Debug, Clone)]
enum Op {
    /// Something happens to the graph's `n`th entity.
    GraphAdd(usize, String),
    GraphRename(usize, String),
    GraphDelete(usize),
    /// Something happens to the phone's `n`th row.
    PhoneAdd(usize, String),
    PhoneRemoveFirst(usize),
    PhoneRename(usize, String),
    PhoneDelete(usize),
    PhoneCreate(String, String),
    Sync,
}

fn op() -> impl Strategy<Value = Op> {
    let value = "[a-z0-9 ]{1,6}".prop_filter("something", |s| !s.trim().is_empty());
    prop_oneof![
        (0..4usize, value.clone()).prop_map(|(i, v)| Op::GraphAdd(i, v)),
        (0..4usize, value.clone()).prop_map(|(i, v)| Op::GraphRename(i, v)),
        (0..4usize).prop_map(Op::GraphDelete),
        (0..4usize, value.clone()).prop_map(|(i, v)| Op::PhoneAdd(i, v)),
        (0..4usize).prop_map(Op::PhoneRemoveFirst),
        (0..4usize, value.clone()).prop_map(|(i, v)| Op::PhoneRename(i, v)),
        (0..4usize).prop_map(Op::PhoneDelete),
        (value.clone(), value).prop_map(|(n, v)| Op::PhoneCreate(n, v)),
        Just(Op::Sync),
    ]
}

proptest! {
    /// Whatever happens on either side, a reconcile ends with the phone
    /// showing exactly the graph's contacts and nothing left to do, and
    /// another one touches nothing.
    #[test]
    fn it_always_converges(ops in proptest::collection::vec(op(), 0..14)) {
        let graph = graph()?;
        let mut people = vec![
            person(&graph, "Ann", "mobile:: 1")?,
            person(&graph, "Ben", "email:: b@x.example")?,
            person(&graph, "Cat", "phone:: 3\nnickname:: C")?,
        ];
        let mut phone = Phone::default();
        sync(&graph, &mut phone);

        for op in ops {
            let rows: Vec<String> = phone.rows.keys().cloned().collect();
            match op {
                Op::GraphAdd(i, v) => {
                    if let Some(&id) = people.get(i).filter(|&&id| graph.contains(id)) {
                        // There is no block left once its lines are all gone.
                        match graph.properties_block(id)? {
                            Some(block) => {
                                let text = graph.text(block)?;
                                graph.set_text(block, &format!("{text}\nphone:: {v}"))?;
                            }
                            None => {
                                graph.create_block(Some(id), Some(0), &format!("phone:: {v}"))?;
                            }
                        }
                    }
                }
                Op::GraphRename(i, v) => {
                    if let Some(&id) = people.get(i).filter(|&&id| graph.contains(id)) {
                        graph.rename_entity(id, v.trim())?;
                    }
                }
                Op::GraphDelete(i) => {
                    if let Some(&id) = people.get(i).filter(|&&id| graph.contains(id)) {
                        graph.delete_block(id)?;
                    }
                }
                Op::PhoneAdd(i, v) => {
                    if let Some(row) = rows.get(i) {
                        phone.edit(row, |c| c.entries.push(entry(F::Phone, Some("mobile"), v.trim())));
                    }
                }
                Op::PhoneRemoveFirst(i) => {
                    if let Some(row) = rows.get(i) {
                        phone.edit(row, |c| {
                            if !c.entries.is_empty() {
                                c.entries.remove(0);
                            }
                        });
                    }
                }
                Op::PhoneRename(i, v) => {
                    if let Some(row) = rows.get(i) {
                        phone.edit(row, |c| c.name = v.trim().to_owned());
                    }
                }
                Op::PhoneDelete(i) => {
                    if let Some(row) = rows.get(i) {
                        phone.delete(row);
                    }
                }
                Op::PhoneCreate(name, v) => {
                    phone.create(Contact {
                        id: String::new(),
                        name: name.trim().to_owned(),
                        entries: vec![entry(F::Email, None, v.trim())],
                    });
                }
                Op::Sync => { sync(&graph, &mut phone); }
            }
            people.retain(|&id| graph.contains(id));
        }

        sync(&graph, &mut phone);
        prop_assert!(phone.rows.values().all(|r| !r.dirty && !r.deleted));
        prop_assert_eq!(phone.shown(), current(&graph));
        let held: Vec<_> = phone.rows.values().map(|r| r.held.clone()).collect();
        let mut held = held;
        held.sort_by(|a, b| a.id.cmp(&b.id));
        prop_assert_eq!(held, current(&graph), "what it was handed is what it shows");

        phone.log.clear();
        let again = sync(&graph, &mut phone);
        prop_assert_eq!(again, Report::default());
        prop_assert_eq!(phone.log, Vec::<String>::new());
    }
}
