//! Keeping a store of contacts, which is somebody else's copy, in step with
//! the graph.
//!
//! The store is a phone's contacts: it keeps a row for each contact, can say
//! which rows were edited since it was last told, and remembers each contact
//! as it was handed over, so that an edit can be told from what was already
//! there. [`reconcile`] takes the edits into the graph, then makes the store
//! match the graph.

use std::collections::HashMap;

use quiddity_core::{BlockId, Graph, Index};
use serde::{Deserialize, Serialize};

use crate::model::{Contact, Entry, contact_of, contacts};
use crate::write::{apply, forget};

/// A row of the store, which was edited, made or deleted since it was last put.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edited {
    /// The store's own name for the row.
    pub row: String,
    /// The entity it was handed over as, if it was handed over: a row made in
    /// the store has none.
    pub id: Option<String>,
    /// The contact as it was handed over.
    pub base: Option<Contact>,
    /// The contact now, in the store.
    pub contact: Contact,
    /// Deleted in the store.
    pub deleted: bool,
}

/// A row of the store, as it was handed over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Held {
    pub row: String,
    pub contact: Contact,
}

/// Somebody's copy of the contacts.
pub trait Store {
    type Error: std::error::Error + 'static;

    /// The rows that were edited, made or deleted since they were last put.
    fn edited(&mut self) -> Result<Vec<Edited>, Self::Error>;

    /// Every row that was handed over, and as what.
    fn held(&mut self) -> Result<Vec<Held>, Self::Error>;

    /// Makes a row, or replaces `row`, with `contact`: the row is no longer
    /// edited, and `contact` is what it was handed over as. Returns the
    /// row's name.
    fn put(&mut self, row: Option<&str>, contact: &Contact) -> Result<String, Self::Error>;

    /// Removes a row.
    fn remove(&mut self, row: &str) -> Result<(), Self::Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum Error<E: std::error::Error + 'static> {
    #[error("{0}")]
    Graph(#[from] quiddity_core::Error),
    #[error("the contacts store failed: {0}")]
    Store(#[source] E),
}

/// What [`reconcile`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Whether the graph was changed, which is for the caller to save and
    /// send on.
    pub changed: bool,
    /// Entities made from contacts made in the store.
    pub made: usize,
    /// Rows put, because they were new or the graph's contact differed.
    pub put: usize,
    /// Rows removed, because the contact was deleted in the store or isn't
    /// one any longer.
    pub removed: usize,
    /// Entries that were in the store and are not in the graph, because no
    /// property fills their field, with the name of the contact.
    pub dropped: Vec<(String, Entry)>,
}

/// Takes what was edited in the store into the graph, then makes the store
/// have exactly the graph's contacts.
///
/// Edits go first, as a three-way merge ([`apply`]), so that what the graph
/// gained since it handed a contact over survives an edit made to an older
/// copy, and the store is then given the result.
pub fn reconcile<S: Store>(graph: &Graph, store: &mut S) -> Result<Report, Error<S::Error>> {
    let mut report = Report::default();
    let edits = store.edited().map_err(Error::Store)?;
    if !edits.is_empty() {
        let index = Index::build(graph)?;
        for edit in edits {
            take(graph, &index, store, edit, &mut report)?;
        }
    }

    let index = Index::build(graph)?;
    let wanted = contacts(&index);
    let held: HashMap<String, Held> = store
        .held()
        .map_err(Error::Store)?
        .into_iter()
        .filter(|h| !h.contact.id.is_empty())
        .map(|h| (h.contact.id.clone(), h))
        .collect();

    for contact in &wanted {
        match held.get(&contact.id) {
            Some(h) if h.contact == *contact => {}
            Some(h) => {
                store.put(Some(&h.row), contact).map_err(Error::Store)?;
                report.put += 1;
            }
            None => {
                store.put(None, contact).map_err(Error::Store)?;
                report.put += 1;
            }
        }
    }
    let kept: std::collections::HashSet<_> = wanted.iter().map(|c| c.id.as_str()).collect();
    for (id, h) in &held {
        if !kept.contains(id.as_str()) {
            store.remove(&h.row).map_err(Error::Store)?;
            report.removed += 1;
        }
    }
    Ok(report)
}

/// One edit in the store, made to the graph. The row it came from is put or
/// removed after, by the pass over the whole store, unless it has no entity
/// yet to be found by.
fn take<S: Store>(
    graph: &Graph,
    index: &Index,
    store: &mut S,
    edit: Edited,
    report: &mut Report,
) -> Result<(), Error<S::Error>> {
    let id: Option<BlockId> = edit.id.as_deref().and_then(|id| id.parse().ok());
    if edit.deleted {
        if let Some(id) = id {
            report.changed |= forget(graph, index, id)?;
        }
        store.remove(&edit.row).map_err(Error::Store)?;
        report.removed += 1;
        return Ok(());
    }

    let applied = apply(graph, index, id, edit.base.as_ref(), &edit.contact)?;
    report.changed |= applied.changed;
    report.dropped.extend(
        applied
            .dropped
            .into_iter()
            .map(|entry| (edit.contact.name.clone(), entry)),
    );
    match applied.id {
        // The entity is the row's now, however it got here: put it as what
        // the graph says it is, which also clears what was edited.
        Some(entity) => {
            if id.is_none() || Some(entity) != id {
                report.made += 1;
            }
            let fresh = Index::build(graph)?;
            match contact_of(&fresh, entity) {
                Some(contact) => {
                    store.put(Some(&edit.row), &contact).map_err(Error::Store)?;
                    report.put += 1;
                }
                // Edited down to nothing: it is no contact any longer.
                None => {
                    store.remove(&edit.row).map_err(Error::Store)?;
                    report.removed += 1;
                }
            }
        }
        // Nothing to make an entity of: the row can't be kept.
        None => {
            store.remove(&edit.row).map_err(Error::Store)?;
            report.removed += 1;
        }
    }
    Ok(())
}
