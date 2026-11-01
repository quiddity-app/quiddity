//! Flags: what doesn't fit a schema. Nothing is rejected; the text is what
//! it is, and these say where it disagrees with what its properties say.

use std::collections::BTreeMap;

use quiddity_syntax::normalise_name;

use crate::schema::{Cardinality, Mismatch, coerce};
use crate::{BlockId, Identity, Index};

/// Something about an entity that doesn't fit the schema of a property.
/// Keys are as written, so the UI can find what to mark.
#[derive(Debug, Clone, PartialEq)]
pub enum Flag {
    /// A value that isn't of the property's `##value-type`.
    Mismatch { key: String, mismatch: Mismatch },
    /// A reference to an entity that the property's `##values` doesn't find.
    NotInValues { key: String, target: BlockId },
    /// A property on an entity its `##applies-to` doesn't find.
    NotApplicable { key: String },
    /// More or fewer values than `##cardinality` allows. A property with
    /// `##applies-to` is counted on every entity it applies to, so one that
    /// is missing is flagged too.
    Cardinality {
        key: String,
        count: usize,
        allowed: Cardinality,
    },
    /// An entity among its own values of a transitive property.
    Cycle { key: String },
    /// A `##key` Quiddity doesn't know.
    UnknownMeta { key: String },
    /// A meta-property whose parameter can't be read, or that has one when
    /// it shouldn't, or none when it should.
    BadMeta { key: String, message: String },
    /// Meta-properties on an entity that doesn't define a property, where
    /// they mean nothing.
    MetaOutsideProperty,
    /// A property entity named for another property's inverse label, which
    /// it would be confused with in queries.
    InverseClash { label: String, property: String },
}

impl Index {
    /// What doesn't fit about an entity's own properties and meta-properties.
    pub fn flags(&self, id: BlockId) -> Vec<Flag> {
        let id = self.canonical(id);
        let Some(info) = self.entity(id) else {
            return Vec::new();
        };
        let mut flags = Vec::new();

        let mut written: BTreeMap<String, &str> = BTreeMap::new();
        for p in &info.properties {
            let key = normalise_name(&p.key);
            let schema = self.schema(&key);
            if let Some(Err(mismatch)) = p.value.as_ref().map(|v| coerce(v, &schema)) {
                flags.push(Flag::Mismatch {
                    key: p.key.clone(),
                    mismatch,
                });
            }
            written.entry(key).or_insert(&p.key);
        }
        let own = self.own_items(id);

        // Every property written, and every one whose `##applies-to` finds
        // the entity, for cardinality.
        let mut keys: BTreeMap<String, String> = written
            .iter()
            .map(|(k, &w)| (k.clone(), w.to_owned()))
            .collect();
        for (key, schema) in &self.schemas {
            if schema
                .applies_to
                .as_ref()
                .is_some_and(|q| self.matches_query(id, q))
            {
                keys.entry(key.clone()).or_insert_with(|| key.clone());
            }
        }
        for (key, as_written) in &keys {
            let schema = self.schema(key);
            let items = own.get(key).map(Vec::as_slice).unwrap_or_default();
            if let Some(values) = &schema.values {
                for t in items.iter().filter_map(|i| i.target) {
                    if !self.matches_query(t, values) {
                        flags.push(Flag::NotInValues {
                            key: as_written.clone(),
                            target: t,
                        });
                    }
                }
            }
            let applies = schema
                .applies_to
                .as_ref()
                .is_none_or(|q| self.matches_query(id, q));
            if !applies {
                flags.push(Flag::NotApplicable {
                    key: as_written.clone(),
                });
            }
            if applies && !schema.cardinality.allows(items.len()) {
                flags.push(Flag::Cardinality {
                    key: as_written.clone(),
                    count: items.len(),
                    allowed: schema.cardinality,
                });
            }
        }

        for (key, items) in self.effective.get(&id).into_iter().flatten() {
            if self.schema(key).transitive && items.iter().any(|i| i.target == Some(id)) {
                let key = written.get(key).map_or(key.as_str(), |w| w).to_owned();
                flags.push(Flag::Cycle { key });
            }
        }

        let defines = matches!(info.identity(), Some(Identity::Property(_)));
        if !info.meta.is_empty() && !defines {
            flags.push(Flag::MetaOutsideProperty);
        }
        if defines {
            flags.extend(self.meta_flags.get(&id).into_iter().flatten().cloned());
            let name = normalise_name(&info.name);
            if let Some(property) = self.inverse_labels().remove(&name)
                && property != name
            {
                flags.push(Flag::InverseClash {
                    label: info.name.clone(),
                    property,
                });
            }
        }
        flags
    }
}
