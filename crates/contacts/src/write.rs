use quiddity_core::contact::{ContactField, ContactMapping};
use quiddity_core::syntax::{Value, format_property_line, parse_block};
use quiddity_core::{BlockId, Graph, Index, Result};

use crate::model::{Contact, Entry, contact_of, flatten};

/// What [`apply`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The entity the contact is, now: the one that was edited, or the one
    /// made. `None` when there was nothing to make it from.
    pub id: Option<BlockId>,
    /// Whether the graph was changed.
    pub changed: bool,
    /// Entries that have no property to go in, because no property fills
    /// their field. They are lost, and the caller can say so.
    pub dropped: Vec<Entry>,
}

/// Makes an edit to a contact, made on a copy of it that was `base` when it
/// was handed over, to the entity it came from.
///
/// It is a three-way merge of the entries: what `desired` has that `base`
/// hadn't is added, and what `base` had that `desired` hasn't is removed, so
/// whatever else the entity has gained since is kept. A rename is made only if
/// the entity still has the name `base` had. With no `id`, or an entity that
/// is gone and edits to make, an entity is made, named as `desired` is, with
/// its entries in the properties that fill their fields.
///
/// `index` must be of `graph` as it is now.
pub fn apply(
    graph: &Graph,
    index: &Index,
    id: Option<BlockId>,
    base: Option<&Contact>,
    desired: &Contact,
) -> Result<Applied> {
    let existing = id
        .map(|id| index.canonical(id))
        .filter(|&id| index.entity(id).is_some());
    let Some(entity) = existing else {
        // Gone, and nothing was changed on the copy: it is the caller's to
        // remove.
        if id.is_some() && base.is_some_and(|b| same_contact(b, desired)) {
            return Ok(Applied {
                id: None,
                changed: false,
                dropped: Vec::new(),
            });
        }
        return create(graph, index, desired);
    };

    let empty = Contact {
        id: String::new(),
        name: String::new(),
        entries: Vec::new(),
    };
    let base = base.unwrap_or(&empty);
    let current = contact_of(index, entity)
        .map(|c| c.entries)
        .unwrap_or_default();

    let removed = minus(&base.entries, &desired.entries);
    let added = minus(&desired.entries, &base.entries);
    let remaining = minus(&current, &removed);
    let mut dropped = Vec::new();
    let mut additions = Vec::new();
    // The copy's count of each entry it added, less what the entity already
    // has: two of one number are two, but one the entity gained by itself
    // isn't added again.
    let wanted = minus(&desired.entries, &remaining)
        .into_iter()
        .filter(|e| added.iter().any(|a| a.same(e)));
    for entry in wanted {
        if entry.value.trim().is_empty() {
            continue;
        }
        match key_for(index, entry.field, entry.label.as_deref()) {
            Some(key) => additions.push((key, entry.value)),
            None => dropped.push(entry),
        }
    }

    let mut changed = rewrite(graph, index, entity, removed, &additions)?;

    let name = desired.name.trim();
    let info = index.entity(entity).expect("found above");
    if !name.is_empty()
        && name != base.name
        && info.name.trim() == base.name
        && info.identity().is_none()
    {
        changed |= graph.rename_entity(entity, name)?;
    }
    Ok(Applied {
        id: Some(entity),
        changed,
        dropped,
    })
}

/// Takes a contact off the entity: removes every value of every property
/// that fills a field, so that it is no contact. The entity stays, with
/// everything else it has, since a contact removed from a phone isn't a
/// reason to lose a page. Says whether anything was removed.
pub fn forget(graph: &Graph, index: &Index, id: BlockId) -> Result<bool> {
    let entity = index.canonical(id);
    let Some(contact) = contact_of(index, entity) else {
        return Ok(false);
    };
    rewrite(graph, index, entity, contact.entries, &[])
}

fn same_contact(a: &Contact, b: &Contact) -> bool {
    a.name.trim() == b.name.trim()
        && a.entries.len() == b.entries.len()
        && minus(&a.entries, &b.entries).is_empty()
}

/// What `a` has that `b` hasn't, counting repeats: `[x, x]` minus `[x]` is
/// `[x]`.
fn minus(a: &[Entry], b: &[Entry]) -> Vec<Entry> {
    let mut unused: Vec<&Entry> = b.iter().collect();
    a.iter()
        .filter(|entry| match unused.iter().position(|e| e.same(entry)) {
            Some(i) => {
                unused.swap_remove(i);
                false
            }
            None => true,
        })
        .cloned()
        .collect()
}

/// The property a new entry goes in: one for its field and label, else one
/// for its field with no label, else any for its field. Only one whose name
/// can be written as a property's key counts.
fn key_for(index: &Index, field: ContactField, label: Option<&str>) -> Option<String> {
    let of_field = || {
        index
            .contact_keys()
            .iter()
            .filter(|(key, m)| m.field == field && writable(key))
    };
    of_field()
        .find(|(_, m)| ContactMapping::same_label(m.label.as_deref(), label))
        .or_else(|| of_field().find(|(_, m)| m.label.is_none()))
        .or_else(|| of_field().next())
        .map(|(key, _)| key.clone())
}

/// Whether a property of this name reads back from a line of its own.
fn writable(key: &str) -> bool {
    let line = format_property_line(key, Some(&Value::Text("x".to_owned())));
    let properties = parse_block(&line).properties;
    matches!(properties.as_slice(), [p] if p.key == key)
}

fn create(graph: &Graph, index: &Index, desired: &Contact) -> Result<Applied> {
    let mut lines = Vec::new();
    let mut dropped = Vec::new();
    for entry in desired
        .entries
        .iter()
        .filter(|e| !e.value.trim().is_empty())
    {
        match key_for(index, entry.field, entry.label.as_deref()) {
            Some(key) => lines.push((key, Some(Value::Text(entry.value.clone())))),
            None => dropped.push(entry.clone()),
        }
    }
    // Without a value to fill a property it wouldn't be a contact, and
    // would be gone at the next look.
    let name = desired.name.lines().next().unwrap_or("").trim();
    let name = if name.is_empty() {
        desired.entries.first().map_or("", |e| e.value.as_str())
    } else {
        name
    };
    if lines.is_empty() || name.trim().is_empty() {
        return Ok(Applied {
            id: None,
            changed: false,
            dropped: desired.entries.clone(),
        });
    }
    let lines: Vec<_> = lines.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    let id = graph.create_entity(name.trim(), &lines)?;
    Ok(Applied {
        id: Some(id),
        changed: true,
        dropped,
    })
}

/// Removes from the entity's properties the entries in `removals`, one
/// property value for each, and adds a line for each of `additions`. A list
/// that loses some of its items keeps the rest, in a list; a line that loses
/// all of them goes. Says whether it changed anything.
fn rewrite(
    graph: &Graph,
    index: &Index,
    entity: BlockId,
    mut removals: Vec<Entry>,
    additions: &[(String, String)],
) -> Result<bool> {
    let block = graph.properties_block(entity)?;
    let text = match block {
        Some(block) => graph.text(block)?,
        None => String::new(),
    };

    let mut edits: Vec<(std::ops::Range<usize>, Option<String>)> = Vec::new();
    for p in &parse_block(&text).properties {
        let (Some(mapping), Some(value)) = (index.schema(&p.key).contact.clone(), &p.value) else {
            continue;
        };
        let items = match value {
            Value::List(items) => items.as_slice(),
            single => std::slice::from_ref(single),
        };
        let mut kept = Vec::new();
        for item in items {
            let mut shown = Vec::new();
            flatten(&index.read_days(item), &mut shown);
            let entry = shown.pop().map(|value| Entry {
                field: mapping.field,
                label: mapping.label.clone(),
                value,
            });
            let hit = entry.and_then(|e| removals.iter().position(|r| r.same(&e)));
            match hit {
                Some(i) => {
                    removals.swap_remove(i);
                }
                None => kept.push(item.clone()),
            }
        }
        if kept.len() == items.len() {
            continue;
        }
        let line =
            (!kept.is_empty()).then(|| format_property_line(&p.key, Some(&Value::List(kept))));
        edits.push((p.span.clone(), line));
    }

    // Line by line, so that deleting neighbouring lines can't take the same
    // newline twice.
    let mut new = if edits.is_empty() {
        text.clone()
    } else {
        let mut lines = Vec::new();
        let mut at = 0;
        for line in text.split('\n') {
            let edit = edits.iter().find(|(span, _)| span.start == at);
            match edit {
                Some((_, Some(replacement))) => lines.push(format!(
                    "{replacement}{}",
                    &line[edit.map_or(0, |(span, _)| span.len())..]
                )),
                Some((_, None)) => {}
                None => lines.push(line.to_owned()),
            }
            at += line.len() + 1;
        }
        lines.join("\n")
    };
    if !additions.is_empty() {
        if !new.is_empty() && !new.ends_with('\n') {
            new.push('\n');
        }
        let lines: Vec<_> = additions
            .iter()
            .map(|(key, value)| format_property_line(key, Some(&Value::Text(value.clone()))))
            .collect();
        new.push_str(&lines.join("\n"));
    }
    if new == text {
        return Ok(false);
    }
    match block {
        Some(block) => graph.set_text(block, &new)?,
        None => {
            graph.create_block(Some(entity), Some(0), &new)?;
        }
    }
    Ok(true)
}
