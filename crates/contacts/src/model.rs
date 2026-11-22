use quiddity_core::contact::{ContactField, ContactMapping};
use quiddity_core::syntax::Value;
use quiddity_core::{BlockId, Index};
use serde::{Deserialize, Serialize};

/// One value of a contact: a phone number, an email address and so on.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Entry {
    pub field: ContactField,
    /// What kind of one it is, as the property that holds it says.
    pub label: Option<String>,
    pub value: String,
}

impl Entry {
    /// Whether `other` is the same entry: the same field and label (labels
    /// compared as names are) and the same value.
    pub fn same(&self, other: &Self) -> bool {
        self.field == other.field
            && ContactMapping::same_label(self.label.as_deref(), other.label.as_deref())
            && self.value == other.value
    }
}

/// An entity as a contact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    /// The entity's ID, which stays with it when it is renamed or moved.
    /// Empty for a contact that doesn't come from one yet.
    pub id: String,
    /// The entity's name.
    pub name: String,
    /// In the order they are written: properties as they come in the
    /// entity's properties block, and a property's values as they come.
    pub entries: Vec<Entry>,
}

/// Every contact in the graph, in entity order. Duplicates that have been
/// merged are left out, and so are entities with nothing to show.
pub fn contacts(index: &Index) -> Vec<Contact> {
    index
        .entities()
        .filter_map(|info| contact_of(index, info.id))
        .collect()
}

/// An entity as a contact, if it is one: it has a value for a property that
/// fills a field. Only what the entity writes itself counts: a contact is not
/// made of what it inherits.
pub fn contact_of(index: &Index, id: BlockId) -> Option<Contact> {
    let info = index.entity(id)?;
    let name = info.name.trim();
    let mut entries = Vec::new();
    for p in &info.properties {
        let Some(mapping) = index.schema(&p.key).contact.clone() else {
            continue;
        };
        let Some(value) = &p.value else { continue };
        let mut values = Vec::new();
        flatten(&index.read_days(value), &mut values);
        entries.extend(values.into_iter().map(|value| Entry {
            field: mapping.field,
            label: mapping.label.clone(),
            value,
        }));
    }
    (!entries.is_empty() && !name.is_empty()).then(|| Contact {
        id: id.to_string(),
        name: name.to_owned(),
        entries,
    })
}

/// The text of each item of a value, as a contacts app would show it: a
/// link by its name. Empty items and block references have none.
pub(crate) fn flatten(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Text(s) => out.push(s.trim().to_owned()),
        Value::Link(link) => out.push(link.name.trim().to_owned()),
        Value::BlockRef(_) => {}
        Value::List(items) => items.iter().for_each(|v| flatten(v, out)),
    }
    out.retain(|s| !s.is_empty());
}
