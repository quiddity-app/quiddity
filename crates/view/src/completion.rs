//! What to offer while typing in an entity's properties block: the keys that
//! graph already uses, and the values already given to a key.
//!
//! Both come from what is written, not from a schema, so a graph's own
//! habits are what get completed. They are read from the index, which is
//! the same on every device.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use quiddity_core::syntax::{PropertyStyle, format_value, normalise_name};
use serde::Serialize;

use crate::search::fuzzy_match;
use crate::{Result, View};

/// A property key to offer.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KeyHit {
    /// As the first entity to use it wrote it.
    pub key: String,
    /// How many entities write it.
    pub uses: usize,
}

/// A value to offer for a key.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ValueHit {
    /// As it is written after `key::`.
    pub value: String,
    /// How many entities have it.
    pub uses: usize,
}

impl View {
    /// The property keys to offer for what has been typed, best first and at
    /// most `limit`: every key an entity writes in its properties block, and
    /// every entity that defines a property with `##` lines, used or not.
    /// A key ranks by how well the query matches it, then by how many entities
    /// use it, then by name. With nothing typed, the most used come first.
    pub fn property_keys(&self, query: &str, limit: usize) -> Result<Vec<KeyHit>> {
        let index = self.index()?;
        // The key as first written, by normalised key, and who writes it.
        let mut keys: BTreeMap<String, (String, usize)> = BTreeMap::new();
        for entity in index.entities() {
            if index.canonical(entity.id) != entity.id {
                continue;
            }
            let mut seen = Vec::new();
            for property in entity
                .properties
                .iter()
                .filter(|p| p.style == PropertyStyle::Line)
            {
                let key = normalise_name(&property.key);
                if key.is_empty() || seen.contains(&key) {
                    continue;
                }
                seen.push(key.clone());
                keys.entry(key)
                    .or_insert_with(|| (property.key.trim().to_owned(), 0))
                    .1 += 1;
            }
            if !entity.meta.is_empty() && index.defines_property(entity.id) {
                keys.entry(normalise_name(&entity.name))
                    .or_insert_with(|| (entity.name.clone(), 0));
            }
        }
        let mut hits: Vec<(i32, KeyHit)> = keys
            .into_values()
            .filter_map(|(key, uses)| {
                // A capital in a query must start a word, which suits titles
                // but not keys, which are typed in any case.
                let score = if query.trim().is_empty() {
                    0
                } else {
                    fuzzy_match(&query.to_lowercase(), &key)?.score
                };
                Some((score, KeyHit { key, uses }))
            })
            .collect();
        hits.sort_by(|(a, x), (b, y)| {
            b.cmp(a)
                .then(y.uses.cmp(&x.uses))
                .then_with(|| normalise_name(&x.key).cmp(&normalise_name(&y.key)))
        });
        hits.truncate(limit);
        Ok(hits.into_iter().map(|(_, hit)| hit).collect())
    }

    /// The values already given to the property `key`, to offer for what has
    /// been typed after `key::`: those that contain it, ignoring case, the
    /// most used first and at most `limit`. A value is one as it is written
    /// (a list is one value), counted once for each entity that has it.
    pub fn property_values(&self, key: &str, query: &str, limit: usize) -> Result<Vec<ValueHit>> {
        let index = self.index()?;
        let key = normalise_name(key);
        let want = query.trim().to_lowercase();
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for entity in index.entities() {
            if index.canonical(entity.id) != entity.id {
                continue;
            }
            let mut mine: Vec<String> = Vec::new();
            for property in entity
                .properties
                .iter()
                .filter(|p| p.style == PropertyStyle::Line && normalise_name(&p.key) == key)
            {
                let Some(value) = &property.value else {
                    continue;
                };
                let written = format_value(value);
                if !mine.contains(&written) {
                    mine.push(written);
                }
            }
            for written in mine {
                if written.to_lowercase().contains(&want) {
                    *counts.entry(written).or_default() += 1;
                }
            }
        }
        let mut hits: Vec<ValueHit> = counts
            .into_iter()
            .map(|(value, uses)| ValueHit { value, uses })
            .collect();
        hits.sort_by_key(|h| (Reverse(h.uses), h.value.to_lowercase()));
        hits.truncate(limit);
        Ok(hits)
    }
}
