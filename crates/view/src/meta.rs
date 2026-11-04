//! What meta-properties add to an entity's page: values it inherits, the
//! relations that point at it, and what doesn't fit a schema.

use std::collections::{BTreeMap, BTreeSet};

use quiddity_core::syntax::{Value, normalise_name};
use quiddity_core::{BlockId, Flag, Index};
use serde::Serialize;

use crate::query::{EntityRef, Property, PropertyValue, ResolvedLink, add_link, property_value};
use crate::{Result, View};

/// How many entities an inverse tree shows before it stops expanding.
pub const INVERSE_LIMIT: usize = 100;

/// A value an entity has from another.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Inherited {
    /// As the property's entity names it, or as it is written where it
    /// comes from.
    pub key: String,
    /// `None` for a property with no value, such as a tag.
    pub value: Option<PropertyValue>,
    /// The entity it is written on.
    pub from: EntityRef,
}

/// The entities related to one the other way round, under an inverse label.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Inverse {
    /// As written in `##inverse(…)`.
    pub label: String,
    pub tree: Vec<InverseNode>,
}

/// An entity in an inverse tree, with those related to it in turn.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InverseNode {
    pub entity: EntityRef,
    pub children: Vec<InverseNode>,
    /// How many of its children aren't shown, because the tree reached its
    /// limit. They are on its own page.
    pub hidden: usize,
}

/// A flag in words, and the key of the chip it belongs on, if any.
fn describe(index: &Index, flag: &Flag) -> (Option<(String, bool)>, String) {
    let title = |id: BlockId| {
        index
            .entity(id)
            .map_or_else(String::new, |e| e.name.clone())
    };
    match flag {
        Flag::Mismatch { key, mismatch } => (
            Some((key.clone(), false)),
            format!("Not a {}", mismatch.expected),
        ),
        Flag::NotInValues { key, target } => (
            Some((key.clone(), false)),
            format!("{} isn't one of the values it allows", title(*target)),
        ),
        Flag::NotApplicable { key } => (
            Some((key.clone(), false)),
            "Doesn't apply to this entity".to_owned(),
        ),
        Flag::Cardinality {
            key,
            count: 0,
            allowed,
        } => (None, format!("Should have {key}: {allowed}")),
        Flag::Cardinality {
            key,
            count,
            allowed,
        } => (
            Some((key.clone(), false)),
            format!("{count} values, where {allowed} are allowed"),
        ),
        Flag::Cycle { key } => (
            Some((key.clone(), false)),
            "Leads back to this entity".to_owned(),
        ),
        Flag::UnknownMeta { key } => (
            Some((key.clone(), true)),
            "Not a meta-property Quiddity knows".to_owned(),
        ),
        Flag::BadMeta { key, message } => (Some((key.clone(), true)), message.clone()),
        Flag::MetaOutsideProperty => (
            None,
            "Meta-properties mean something only on a property's entity, with type:: property"
                .to_owned(),
        ),
        Flag::InverseClash { label, property } => (
            None,
            format!("{label} is also the inverse of {property}, which queries for it will mean"),
        ),
    }
}

impl View {
    /// Puts each of an entity's flags on the chip it is about, in its
    /// properties block, and returns those that have none.
    pub(crate) fn flag(
        &self,
        id: BlockId,
        mut chips: Option<&mut Vec<Property>>,
    ) -> Result<Vec<String>> {
        let index = self.index()?;
        let mut loose = Vec::new();
        for flag in index.flags(id) {
            let (about, message) = describe(&index, &flag);
            let chip = about.and_then(|(key, meta)| {
                let key = normalise_name(&key);
                chips
                    .as_deref_mut()?
                    .iter_mut()
                    .find(|p| p.meta == meta && normalise_name(&p.key) == key)
            });
            match chip {
                Some(chip) => chip.flags.push(message),
                None => loose.push(message),
            }
        }
        Ok(loose)
    }

    /// The values an entity has from others, with the links in them added
    /// to `links`.
    pub(crate) fn inherited(
        &self,
        id: BlockId,
        links: &mut BTreeMap<String, ResolvedLink>,
    ) -> Result<Vec<Inherited>> {
        let index = self.index()?;
        let mut out = Vec::new();
        for (key, items) in index.effective_properties(id) {
            let shown = index
                .property_entity(&key)
                .and_then(|p| index.entity(p))
                .map_or(key.clone(), |e| e.name.clone());
            for item in items {
                let Some(from) = item.from else { continue };
                if let Some(Value::Link(link)) = &item.value {
                    add_link(&index, link, links);
                }
                out.push(Inherited {
                    key: shown.clone(),
                    value: item.value.as_ref().map(property_value),
                    from: EntityRef {
                        id: from.to_string(),
                        title: index
                            .entity(from)
                            .map_or_else(String::new, |e| e.name.clone()),
                    },
                });
            }
        }
        Ok(out)
    }

    /// The inverse relations pointing at an entity, each as a tree of the
    /// entities written as related to it, then to them, and so on, until
    /// [`INVERSE_LIMIT`] entities are shown.
    pub(crate) fn inverses(&self, id: BlockId) -> Result<Vec<Inverse>> {
        let index = self.index()?;
        let mut out = Vec::new();
        for (label, property) in index.inverse_labels() {
            if index.inverse(&label, id, true).is_empty() {
                continue;
            }
            let shown = index
                .schema(&property)
                .inverse
                .clone()
                .unwrap_or(label.clone());
            let mut budget = INVERSE_LIMIT;
            let mut path = BTreeSet::from([id]);
            let (tree, _) = inverse_tree(&index, &label, id, &mut budget, &mut path);
            out.push(Inverse { label: shown, tree });
        }
        Ok(out)
    }
}

/// The entities related to `id` under `label`, and how many more there
/// are than the budget allows. Siblings are all shown before anything
/// beneath them, so a deep branch can't use up the limit before the rest of
/// its level. An entity already on the path from the top isn't repeated
/// beneath itself.
fn inverse_tree(
    index: &Index,
    label: &str,
    id: BlockId,
    budget: &mut usize,
    path: &mut BTreeSet<BlockId>,
) -> (Vec<InverseNode>, usize) {
    let related: Vec<BlockId> = index
        .inverse(label, id, true)
        .into_iter()
        .filter(|e| !path.contains(e))
        .collect();
    let shown = related.len().min(*budget);
    *budget -= shown;
    let mut nodes = Vec::new();
    for &e in &related[..shown] {
        path.insert(e);
        let (children, hidden) = inverse_tree(index, label, e, budget, path);
        path.remove(&e);
        nodes.push(InverseNode {
            entity: EntityRef {
                id: e.to_string(),
                title: index.entity(e).map_or_else(String::new, |i| i.name.clone()),
            },
            children,
            hidden,
        });
    }
    (nodes, related.len() - shown)
}
