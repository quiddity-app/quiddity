//! Effective properties: what an entity has once meta-properties are taken
//! into account. Its own values, values a transitive property reaches,
//! properties it inherits, and the other side of symmetric relations.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use quiddity_syntax::{Link, Value, normalise_name};

use crate::schema::ValueType;
use crate::{BlockId, Index, Resolution, canonical_value};

/// One value of a property, as an entity effectively has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// As written: one value, never a list. `None` for a property written
    /// without one, such as a tag.
    pub value: Option<Value>,
    /// The entity it refers to, if it is a reference that resolves to one.
    pub target: Option<BlockId>,
    /// The entity it is written on, if not this one: where an inherited or
    /// transitive value comes from.
    pub from: Option<BlockId>,
}

impl Item {
    /// What makes two items the same value: the entity a reference refers
    /// to, or the text compared as names are.
    fn identity(&self) -> Identity {
        match self.target {
            Some(t) => (Some(t), None),
            None => (None, self.value.as_ref().map(canonical_value)),
        }
    }
}

/// What makes two values of a property the same: see [`Item::identity`].
type Identity = (Option<BlockId>, Option<String>);

/// Properties never inherited: what identifies an entity.
const NOT_INHERITED: &[&str] = &["title", "alias", "id", "date"];

/// An entity's effective properties, by normalised key.
pub(crate) type Effective = BTreeMap<String, Vec<Item>>;

impl Index {
    /// An entity's own values, by normalised key, with lists split into
    /// their items.
    pub(crate) fn own_items(&self, id: BlockId) -> Effective {
        let mut own = Effective::new();
        let Some(info) = self.entity(id) else {
            return own;
        };
        for p in &info.properties {
            let key = normalise_name(&p.key);
            let reference = self.schema(&key).value_type == ValueType::Ref;
            let items = own.entry(key).or_default();
            let values: Vec<Option<&Value>> = match &p.value {
                None => vec![None],
                Some(Value::List(list)) => list.iter().map(Some).collect(),
                Some(v) => vec![Some(v)],
            };
            for value in values {
                items.push(Item {
                    target: value.and_then(|v| self.target(v, reference)),
                    value: value.cloned(),
                    from: None,
                });
            }
        }
        own
    }

    /// The entity a value refers to. A link always refers; bare text only in
    /// a reference property, where `type:: person` means `[[person]]`. An
    /// ambiguous or missing link refers to nothing.
    fn target(&self, value: &Value, reference: bool) -> Option<BlockId> {
        let link = match value {
            Value::Link(link) => link.clone(),
            Value::Text(name) if reference => Link {
                name: name.clone(),
                qualifiers: Vec::new(),
            },
            _ => return None,
        };
        match self.resolve(&link) {
            Resolution::Unique(id) => Some(self.canonical(id)),
            _ => None,
        }
    }

    /// Works out every entity's effective properties, repeating until
    /// nothing changes, so that chains of any length and cycles are fine.
    pub(crate) fn compute_effective(&mut self) {
        let ids: Vec<BlockId> = self.entities().map(|e| e.id).collect();
        let mut effective: HashMap<BlockId, Effective> =
            ids.iter().map(|&id| (id, self.own_items(id))).collect();
        let own_keys: HashMap<BlockId, BTreeSet<String>> = effective
            .iter()
            .map(|(&id, eff)| (id, eff.keys().cloned().collect()))
            .collect();
        let mut seen: HashMap<(BlockId, String), HashSet<Identity>> = HashMap::new();
        for (&id, eff) in &effective {
            for (key, items) in eff {
                seen.insert(
                    (id, key.clone()),
                    items.iter().map(Item::identity).collect(),
                );
            }
        }

        let mut changed = true;
        while changed {
            let mut additions: Vec<(BlockId, String, Item)> = Vec::new();
            for &id in &ids {
                for (key, items) in &effective[&id] {
                    let schema = self.schema(key);
                    let targets: BTreeSet<BlockId> =
                        items.iter().filter_map(|i| i.target).collect();
                    for &t in &targets {
                        let Some(theirs) = effective.get(&t) else {
                            continue;
                        };
                        if schema.transitive {
                            for item in theirs.get(key).into_iter().flatten() {
                                additions.push((
                                    id,
                                    key.clone(),
                                    Item {
                                        from: item.from.or(Some(t)),
                                        ..item.clone()
                                    },
                                ));
                            }
                        }
                        if schema.inherits {
                            for (k, their_items) in theirs {
                                if k == key
                                    || NOT_INHERITED.contains(&k.as_str())
                                    || own_keys[&id].contains(k)
                                {
                                    continue;
                                }
                                for item in their_items {
                                    additions.push((
                                        id,
                                        k.clone(),
                                        Item {
                                            from: item.from.or(Some(t)),
                                            ..item.clone()
                                        },
                                    ));
                                }
                            }
                        }
                        if schema.symmetric {
                            let name = self.entity(id).map(|e| e.name.clone()).unwrap_or_default();
                            additions.push((
                                t,
                                key.clone(),
                                Item {
                                    value: Some(Value::Link(Link {
                                        name,
                                        qualifiers: Vec::new(),
                                    })),
                                    target: Some(id),
                                    from: Some(id),
                                },
                            ));
                        }
                    }
                }
            }
            changed = false;
            for (id, key, item) in additions {
                let new = seen
                    .entry((id, key.clone()))
                    .or_default()
                    .insert(item.identity());
                if new {
                    effective
                        .entry(id)
                        .or_default()
                        .entry(key)
                        .or_default()
                        .push(item);
                }
                changed |= new;
            }
        }

        // The other way round: for each inverse label, who refers to whom,
        // and whether directly by any of the properties with that label.
        let mut inverse: HashMap<(String, BlockId), BTreeMap<BlockId, bool>> = HashMap::new();
        for (&id, eff) in &effective {
            for (key, items) in eff {
                let Some(label) = self.schema(key).inverse.as_deref().map(normalise_name) else {
                    continue;
                };
                for item in items {
                    if let Some(t) = item.target {
                        *inverse
                            .entry((label.clone(), t))
                            .or_default()
                            .entry(id)
                            .or_default() |= item.from.is_none();
                    }
                }
            }
        }
        self.effective = effective;
        self.inverse = inverse
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().collect()))
            .collect();
    }

    /// The values of `key` an entity effectively has: its own, those a
    /// transitive property reaches, inherited ones, and, for an inverse
    /// label, the entities whose property refers to it.
    pub fn effective_values(&self, id: BlockId, key: &str) -> Vec<Item> {
        let key = normalise_name(key);
        let id = self.canonical(id);
        let mut items: Vec<Item> = self
            .effective
            .get(&id)
            .and_then(|eff| eff.get(&key))
            .cloned()
            .unwrap_or_default();
        for &(e, _) in self.inverse.get(&(key, id)).into_iter().flatten() {
            items.push(Item {
                value: self.entity(e).map(|info| {
                    Value::Link(Link {
                        name: info.name.clone(),
                        qualifiers: Vec::new(),
                    })
                }),
                target: Some(e),
                from: Some(e),
            });
        }
        items
    }

    /// Every property an entity effectively has, by normalised key, not
    /// counting inverse labels.
    pub fn effective_properties(&self, id: BlockId) -> Effective {
        self.effective
            .get(&self.canonical(id))
            .cloned()
            .unwrap_or_default()
    }

    /// The entities on the other side of the relation `label` names, as an
    /// inverse: with `##inverse(children)` on `parent`, `inverse("children",
    /// scotland)` is everything with Scotland as a parent. `direct` leaves
    /// out those reached only through a transitive or inherited value.
    pub fn inverse(&self, label: &str, id: BlockId, direct: bool) -> Vec<BlockId> {
        self.inverse
            .get(&(normalise_name(label), self.canonical(id)))
            .into_iter()
            .flatten()
            .filter(|(_, d)| *d || !direct)
            .map(|&(e, _)| e)
            .collect()
    }

    /// The inverse labels of every property, with the property each belongs
    /// to, by normalised label.
    pub fn inverse_labels(&self) -> BTreeMap<String, String> {
        self.schemas
            .iter()
            .filter_map(|(key, s)| Some((normalise_name(s.inverse.as_deref()?), key.clone())))
            .collect()
    }
}
