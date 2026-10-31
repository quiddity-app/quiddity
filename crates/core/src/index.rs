use std::collections::{BTreeMap, BTreeSet, HashMap};

use jiff::civil::Date;
use quiddity_syntax::{Link, Property, Qualifier, Value, normalise_name, parse_block};

use crate::{BlockId, Graph, Result};

/// What the index knows about one entity.
#[derive(Debug, Clone)]
pub struct EntityInfo {
    pub id: BlockId,
    pub name: String,
    pub properties: Vec<Property>,
}

impl EntityInfo {
    /// Values of every instance of the property `key`.
    pub fn values<'a>(&'a self, key: &str) -> impl Iterator<Item = &'a Value> + 'a {
        let key = normalise_name(key);
        self.properties
            .iter()
            .filter(move |p| normalise_name(&p.key) == key)
            .filter_map(|p| p.value.as_ref())
    }

    /// The entity's types, from its `type` property.
    pub fn types(&self) -> BTreeSet<String> {
        self.values("type")
            .flat_map(|v| match v {
                Value::List(items) => items.iter().collect(),
                v => vec![v],
            })
            .map(canonical_value)
            .collect()
    }

    /// For entities fully identified by their properties, that identity.
    pub fn identity(&self) -> Option<Identity> {
        let types = self.types();
        if types.contains("journal") {
            let date = self.values("date").find_map(|v| match v {
                Value::Text(s) => s.parse::<Date>().ok(),
                _ => None,
            });
            if let Some(date) = date {
                return Some(Identity::Journal(date));
            }
        }
        types
            .contains("property")
            .then(|| Identity::Property(normalise_name(&self.name)))
    }

    /// Whether one of the entity's properties satisfies the qualifier.
    pub fn matches(&self, q: &Qualifier) -> bool {
        let key = normalise_name(&q.key);
        self.properties
            .iter()
            .filter(|p| normalise_name(&p.key) == key)
            .any(|p| match (&q.value, &p.value) {
                (None, _) => true,
                (Some(want), Some(have)) => value_matches(have, want),
                (Some(_), None) => false,
            })
    }
}

/// An identity that at most one entity should have. Entities that share one
/// are duplicates, typically created concurrently on different devices.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Identity {
    /// A journal page, identified by its date.
    Journal(Date),
    /// A property key, identified by its normalised name.
    Property(String),
}

/// The outcome of following a link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Unique(BlockId),
    /// Several entities match; the UI shows a disambiguation page.
    Ambiguous(Vec<BlockId>),
    Missing,
}

/// A block that links to an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backlink {
    pub source: BlockId,
    /// The link matches other entities too.
    pub ambiguous: bool,
}

/// Everything derived from a graph's text: names, properties, links and
/// backlinks. Rebuilt from the graph, never synced.
#[derive(Debug, Default)]
pub struct Index {
    entities: BTreeMap<BlockId, EntityInfo>,
    by_name: HashMap<String, BTreeSet<BlockId>>,
    by_identity: BTreeMap<Identity, Vec<BlockId>>,
    /// Duplicates, mapped to the entity they are merged into.
    canonical: HashMap<BlockId, BlockId>,
    by_ref_id: HashMap<String, BlockId>,
    backlinks: HashMap<BlockId, Vec<Backlink>>,
}

impl Index {
    pub fn build(graph: &Graph) -> Result<Self> {
        let mut index = Self::default();
        for id in graph.roots() {
            let info = EntityInfo {
                id,
                name: graph.entity_name(id)?,
                properties: graph.entity_properties(id)?,
            };
            if let Some(identity) = info.identity() {
                index.by_identity.entry(identity).or_default().push(id);
            }
            index.entities.insert(id, info);
        }
        for group in index.by_identity.values_mut() {
            group.sort();
            for &dup in &group[1..] {
                index.canonical.insert(dup, group[0]);
            }
        }
        for info in index.entities.values() {
            if index.canonical.contains_key(&info.id) {
                continue;
            }
            let aliases = info.values("alias").flat_map(|v| match v {
                Value::List(items) => items.iter().collect(),
                v => vec![v],
            });
            let names =
                std::iter::once(normalise_name(&info.name)).chain(aliases.map(canonical_value));
            for name in names.filter(|n| !n.is_empty()) {
                index.by_name.entry(name).or_default().insert(info.id);
            }
        }

        let mut references = Vec::new();
        for block in graph.blocks() {
            if graph.parent(block).is_none() {
                continue;
            }
            let parsed = parse_block(&graph.text(block)?);
            for p in &parsed.properties {
                if let (true, Some(Value::Text(id))) = (normalise_name(&p.key) == "id", &p.value) {
                    let entry = index.by_ref_id.entry(id.clone()).or_insert(block);
                    *entry = (*entry).min(block);
                }
            }
            references.extend(parsed.links.into_iter().map(|link| (block, link)));
        }
        for (source, link) in references {
            let (targets, ambiguous) = match index.resolve(&link) {
                Resolution::Unique(id) => (vec![id], false),
                Resolution::Ambiguous(ids) => (ids, true),
                Resolution::Missing => continue,
            };
            for target in targets {
                index
                    .backlinks
                    .entry(target)
                    .or_default()
                    .push(Backlink { source, ambiguous });
            }
        }
        Ok(index)
    }

    pub fn entity(&self, id: BlockId) -> Option<&EntityInfo> {
        self.entities.get(&id)
    }

    /// All entities, duplicates excluded.
    pub fn entities(&self) -> impl Iterator<Item = &EntityInfo> {
        self.entities
            .values()
            .filter(|e| !self.canonical.contains_key(&e.id))
    }

    /// The entity a duplicate has been merged into, or `id` itself.
    pub fn canonical(&self, id: BlockId) -> BlockId {
        self.canonical.get(&id).copied().unwrap_or(id)
    }

    /// Entities with this name or alias.
    pub fn named(&self, name: &str) -> Vec<BlockId> {
        self.by_name
            .get(&normalise_name(name))
            .map(|ids| ids.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Follows a link: entities with the link's name or alias whose
    /// properties match all of the link's qualifiers.
    pub fn resolve(&self, link: &Link) -> Resolution {
        let mut matches: Vec<_> = self
            .named(&link.name)
            .into_iter()
            .filter(|id| {
                let info = &self.entities[id];
                link.qualifiers.iter().all(|q| info.matches(q))
            })
            .collect();
        match matches.len() {
            0 => Resolution::Missing,
            1 => Resolution::Unique(matches[0]),
            _ => {
                matches.sort();
                Resolution::Ambiguous(matches)
            }
        }
    }

    /// The block with this `id` property.
    pub fn resolve_block_ref(&self, id: &str) -> Option<BlockId> {
        self.by_ref_id.get(id).copied()
    }

    /// Blocks linking to `target`.
    pub fn backlinks(&self, target: BlockId) -> &[Backlink] {
        self.backlinks.get(&target).map_or(&[], Vec::as_slice)
    }

    /// The entity with this identity.
    pub fn by_identity(&self, identity: &Identity) -> Option<BlockId> {
        self.by_identity.get(identity).map(|ids| ids[0])
    }

    /// The journal page for `date`.
    pub fn journal(&self, date: Date) -> Option<BlockId> {
        self.by_identity(&Identity::Journal(date))
    }

    /// The entity defining the property `key`. Property names are unique.
    pub fn property_entity(&self, key: &str) -> Option<BlockId> {
        self.by_identity(&Identity::Property(normalise_name(key)))
    }

    /// Groups of entities sharing an identity, lowest ID (the one kept) first.
    pub fn duplicates(&self) -> impl Iterator<Item = &[BlockId]> {
        self.by_identity
            .values()
            .filter(|g| g.len() > 1)
            .map(Vec::as_slice)
    }
}

/// A value's comparison form: names and text compare like names, ignoring
/// case and repeated whitespace.
pub fn canonical_value(value: &Value) -> String {
    match value {
        Value::Text(s) => normalise_name(s),
        Value::Link(link) => normalise_name(&link.name),
        Value::BlockRef(id) => format!("(({id}))"),
        Value::List(items) => {
            let items: Vec<_> = items.iter().map(canonical_value).collect();
            format!("[{}]", items.join(", "))
        }
    }
}

fn value_matches(have: &Value, want: &Value) -> bool {
    match have {
        Value::List(items) if !matches!(want, Value::List(_)) => {
            items.iter().any(|i| value_matches(i, want))
        }
        _ => canonical_value(have) == canonical_value(want),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Option<Value> {
        Some(Value::Text(s.to_owned()))
    }

    // Resolution and backlinks from prose are property-tested against an
    // oracle in tests/index.rs.

    #[test]
    fn links_in_values_and_block_refs() -> Result<()> {
        let graph = Graph::new();
        let jane = graph.create_entity("Jane", &[])?;
        let notes = graph.create_entity(
            "Notes",
            &[(
                "about",
                Some(Value::Link(Link {
                    name: "Jane".into(),
                    qualifiers: vec![],
                })),
            )],
        )?;
        let props = graph.children(notes)[0];
        let inline = graph.create_block(Some(notes), None, "#with([ [[Jane]]])")?;
        let quoted = graph.create_block(Some(notes), None, "Quotable\nid:: abc-123")?;
        let index = Index::build(&graph)?;

        let sources: Vec<_> = index.backlinks(jane).iter().map(|b| b.source).collect();
        assert_eq!(
            sources,
            [props, inline],
            "links in property values are backlinks"
        );
        assert_eq!(index.resolve_block_ref("abc-123"), Some(quoted));
        assert_eq!(index.resolve_block_ref("nope"), None);
        Ok(())
    }

    #[test]
    fn identities() -> Result<()> {
        let graph = Graph::new();
        let day = graph.create_entity(
            "2026-10-25",
            &[("type", text("journal")), ("date", text("2026-10-25"))],
        )?;
        let dup = graph.create_entity(
            "2026-10-25",
            &[("type", text("journal")), ("date", text("2026-10-25"))],
        )?;
        let status = graph.create_entity("Status", &[("type", text("property"))])?;
        graph.create_entity(
            "Not a date",
            &[("type", text("journal")), ("date", text("soon"))],
        )?;
        let index = Index::build(&graph)?;

        let date: Date = "2026-10-25".parse().unwrap();
        assert_eq!(index.journal(date), Some(day));
        assert_eq!(index.canonical(dup), day);
        assert_eq!(index.named("2026-10-25"), vec![day]);
        assert_eq!(index.property_entity("status"), Some(status));
        assert_eq!(
            index.duplicates().collect::<Vec<_>>(),
            vec![&[day, dup][..]]
        );
        Ok(())
    }
}
