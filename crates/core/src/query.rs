//! Answering [`Query`]s against the index.

use quiddity_syntax::{Query, normalise_name};

use crate::{BlockId, Index, Item, Resolution, canonical_value};

impl Index {
    /// The entities a query finds, merged duplicates left out, in ID order.
    pub fn query(&self, query: &Query) -> Vec<BlockId> {
        self.entities()
            .map(|e| e.id)
            .filter(|&id| self.matches_query(id, query))
            .collect()
    }

    /// Whether a query finds the entity. It sees effective properties:
    /// inherited ones, values a transitive property reaches, and inverse
    /// labels.
    pub fn matches_query(&self, id: BlockId, query: &Query) -> bool {
        let id = self.canonical(id);
        match query {
            Query::Name(name) => self.named(name).contains(&id),
            Query::Link(link) => self.resolves_to(link, id),
            Query::Has { key, value } => {
                let items = self.effective_values(id, key);
                match value {
                    None => !items.is_empty(),
                    Some(q) => items.iter().any(|item| self.item_matches(item, q)),
                }
            }
            Query::Not(a) => !self.matches_query(id, a),
            Query::And(a, b) => self.matches_query(id, a) && self.matches_query(id, b),
            Query::Diff(a, b) => self.matches_query(id, a) && !self.matches_query(id, b),
            Query::Or(a, b) => self.matches_query(id, a) || self.matches_query(id, b),
        }
    }

    /// Whether one value of a property matches the query in `#key(query)`.
    /// A name matches the same text or a reference to an entity so called;
    /// anything else matches a reference to an entity it finds.
    fn item_matches(&self, item: &Item, query: &Query) -> bool {
        match query {
            Query::Name(name) => {
                item.value
                    .as_ref()
                    .is_some_and(|v| canonical_value(v) == normalise_name(name))
                    || item.target.is_some_and(|t| self.named(name).contains(&t))
            }
            Query::Link(link) => item.target.is_some_and(|t| self.resolves_to(link, t)),
            Query::Has { .. } => item.target.is_some_and(|t| self.matches_query(t, query)),
            Query::Not(a) => !self.item_matches(item, a),
            Query::And(a, b) => self.item_matches(item, a) && self.item_matches(item, b),
            Query::Diff(a, b) => self.item_matches(item, a) && !self.item_matches(item, b),
            Query::Or(a, b) => self.item_matches(item, a) || self.item_matches(item, b),
        }
    }

    fn resolves_to(&self, link: &quiddity_syntax::Link, id: BlockId) -> bool {
        match self.resolve(link) {
            Resolution::Unique(found) => self.canonical(found) == id,
            Resolution::Ambiguous(found) => found.iter().any(|&f| self.canonical(f) == id),
            Resolution::Missing => false,
        }
    }
}
