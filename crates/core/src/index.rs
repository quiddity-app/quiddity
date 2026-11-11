use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use jiff::civil::Date;
use quiddity_syntax::{
    Block, Link, MetaProperty, Property, PropertyStyle, Qualifier, Value, normalise_name,
    parse_block, parse_properties_block,
};

use crate::effective::Effective;
use crate::schema::PropertySchema;
use crate::tags;
use crate::{BlockId, Flag, Graph, Result, Scanned};

/// What the index reads from one block's text.
struct Read {
    /// Its properties and `##` lines, links and block references, with tags of
    /// either form among the properties.
    block: Block,
    /// Everything it links to: its links, and its tags, which are links.
    links: Vec<Link>,
    /// The lines of a properties block that it doesn't read.
    stray: Vec<String>,
}

/// Reads a block's text, as a properties block if it is an entity's first
/// child and starts like one.
fn read_block(text: &str, first_child: bool) -> Read {
    let found = first_child.then(|| parse_properties_block(text)).flatten();
    let (mut block, stray) = match found {
        Some(found) => {
            let stray = found
                .stray
                .iter()
                .map(|r| text[r.clone()].trim().to_owned())
                .collect();
            (found.block, stray)
        }
        None => (parse_block(text), Vec::new()),
    };
    let mut links = block.links.clone();
    links.extend(
        block
            .properties
            .iter()
            .filter(|p| p.style == PropertyStyle::Inline)
            .map(|p| tags::link(&p.key)),
    );
    links.extend(tags::expand(&mut block));
    Read {
        block,
        links,
        stray,
    }
}

/// What the index knows about one entity.
#[derive(Debug, Clone)]
pub struct EntityInfo {
    pub id: BlockId,
    pub name: String,
    pub properties: Vec<Property>,
    /// Meta-property lines in its properties block. They mean something
    /// only on a property entity.
    pub meta: Vec<MetaProperty>,
    /// Lines of its properties block that are no property, tag or `##` line,
    /// as written. They are read as nothing, and flagged.
    pub stray: Vec<String>,
}

impl EntityInfo {
    /// The other names it answers to: the items of every `alias` property,
    /// which may each be a single name or a list of them.
    pub fn aliases(&self) -> Vec<&Value> {
        self.values("alias")
            .flat_map(|v| match v {
                Value::List(items) => items.iter().collect(),
                v => vec![v],
            })
            .collect()
    }

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
        if let Some(date) = crate::journal::date(&self.meta) {
            return Some(Identity::Journal(date));
        }
        self.types()
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
    /// The blocks that are blobs, in document order.
    blobs: Vec<(BlockId, crate::Asset)>,
    /// The `##blob` line of every block that starts with one, as written,
    /// whether or not it can be read.
    blob_lines: Vec<String>,
    backlinks: HashMap<BlockId, Vec<Backlink>>,
    /// The entity (root block) each block is in, itself for a root.
    root_of: HashMap<BlockId, BlockId>,
    /// Where each block is in document order, which is the order of every
    /// list of backlinks.
    position: HashMap<BlockId, usize>,
    /// Schemas of built-in properties and of those with a property entity,
    /// by normalised key.
    pub(crate) schemas: HashMap<String, PropertySchema>,
    /// What couldn't be read of each property entity's meta-properties.
    pub(crate) meta_flags: HashMap<BlockId, Vec<Flag>>,
    pub(crate) effective: HashMap<BlockId, Effective>,
    /// For each inverse label and entity, the entities whose property
    /// refers to it, and whether directly (as written).
    pub(crate) inverse: HashMap<(String, BlockId), Vec<(BlockId, bool)>>,
}

impl Index {
    pub fn build(graph: &Graph) -> Result<Self> {
        Ok(Self::from_scan(&graph.scan()?))
    }

    /// Builds the index from blocks read by [`Graph::scan`], or kept from an
    /// earlier one, so that a caller that already holds the text doesn't ask
    /// Loro for it again.
    pub fn from_scan(blocks: &[Scanned]) -> Self {
        let mut index = Self::default();
        let mut root = None;
        for (at, block) in blocks.iter().enumerate() {
            let entity = *root.get_or_insert(block.id);
            index.position.insert(block.id, at);
            if block.parent.is_none() {
                root = Some(block.id);
                let entity = block.id;
                // The properties block is the first child, if it is one.
                let found = blocks
                    .get(at + 1)
                    .filter(|next| next.parent == Some(entity))
                    .filter(|next| parse_properties_block(&next.text).is_some())
                    .map(|next| read_block(&next.text, true));
                let ((properties, meta), stray) = found.map_or_else(
                    || ((Vec::new(), Vec::new()), Vec::new()),
                    |read| ((read.block.properties, read.block.meta), read.stray),
                );
                let info = EntityInfo {
                    id: entity,
                    name: block.text.lines().next().unwrap_or("").trim().to_owned(),
                    properties,
                    meta,
                    stray,
                };
                if let Some(identity) = info.identity() {
                    index.by_identity.entry(identity).or_default().push(entity);
                }
                index.entities.insert(entity, info);
                index.root_of.insert(entity, entity);
            } else {
                index.root_of.insert(block.id, entity);
            }
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
            let names = std::iter::once(normalise_name(&info.name))
                .chain(info.aliases().into_iter().map(canonical_value));
            for name in names.filter(|n| !n.is_empty()) {
                index.by_name.entry(name).or_default().insert(info.id);
            }
        }

        let mut references = Vec::new();
        for (at, scanned) in blocks.iter().enumerate() {
            if scanned.parent.is_none() {
                continue;
            }
            let block = scanned.id;
            // An entity's first child comes straight after it, and if it is a
            // properties block only its own lines are read.
            let first_child = at
                .checked_sub(1)
                .and_then(|before| blocks.get(before))
                .is_some_and(|before| before.parent.is_none());
            let read = read_block(&scanned.text, first_child);
            if let Some(line) = crate::Asset::line_of(&scanned.text) {
                index.blob_lines.push(line.unwrap_or_default());
                if let Some(Ok(asset)) = crate::Asset::in_block(&scanned.text) {
                    index.blobs.push((block, asset));
                }
            }
            if let Some(id) = crate::ref_id::of(&read.block.meta) {
                let entry = index.by_ref_id.entry(id.to_owned()).or_insert(block);
                *entry = (*entry).min(block);
            }
            references.extend(read.links.into_iter().map(|link| (block, link)));
        }
        for (source, link) in references {
            let Some((targets, ambiguous)) = index.targets(&link) else {
                continue;
            };
            for target in targets {
                index
                    .backlinks
                    .entry(target)
                    .or_default()
                    .push(Backlink { source, ambiguous });
            }
        }
        index.read_schemas();
        index.compute_effective();
        index
    }

    /// What a link refers to, and whether it is ambiguous. `None` if nothing.
    fn targets(&self, link: &Link) -> Option<(Vec<BlockId>, bool)> {
        match self.resolve(link) {
            Resolution::Unique(id) => Some((vec![id], false)),
            Resolution::Ambiguous(ids) => Some((ids, true)),
            Resolution::Missing => None,
        }
    }

    /// Brings the index up to date with a block's text changing from `old`
    /// to `new`, without building it again, and says whether it could.
    ///
    /// Only an ordinary block's backlinks can change this way: entities,
    /// names, properties, schemas and effective properties are all read from
    /// roots and properties blocks. So this declines (and the caller builds
    /// the index again) for a root, for a properties block either side of the
    /// edit, and for a block with an `id` property either side, which would
    /// change which block `((id))` refers to. It also declines a block it
    /// doesn't know, which is one made since the index was built.
    ///
    /// The blocks must be where they were when the index was built: it relies
    /// on that for their order.
    pub fn update_text(&mut self, block: BlockId, old: &str, new: &str) -> bool {
        let known = self.root_of.get(&block).is_some_and(|&root| root != block);
        let properties_block = |text| parse_properties_block(text).is_some();
        // A blob is listed by what its first line says.
        let blob = |text: &str| crate::Asset::in_block(text).is_some();
        if !known || properties_block(old) || properties_block(new) || blob(old) || blob(new) {
            return false;
        }
        let (old, new) = (read_block(old, false), read_block(new, false));
        if [&old, &new]
            .iter()
            .any(|read| crate::ref_id::of(&read.block.meta).is_some())
        {
            return false;
        }

        // Where this block's backlinks go: after those of blocks before it.
        let at = self.position[&block];
        for link in &old.links {
            for target in self.targets(link).map(|(t, _)| t).unwrap_or_default() {
                if let Some(links) = self.backlinks.get_mut(&target) {
                    links.retain(|b| b.source != block);
                }
            }
        }
        for link in &new.links {
            let Some((targets, ambiguous)) = self.targets(link) else {
                continue;
            };
            for target in targets {
                let position = &self.position;
                let links = self.backlinks.entry(target).or_default();
                // After this block's own earlier links, so they keep their order.
                let after = links.partition_point(|b| position[&b.source] <= at);
                links.insert(
                    after,
                    Backlink {
                        source: block,
                        ambiguous,
                    },
                );
            }
        }
        true
    }

    /// Reads each property entity's meta-properties over the built-in
    /// schemas.
    fn read_schemas(&mut self) {
        let mut schemas = HashMap::new();
        let mut flags = HashMap::new();
        for (identity, ids) in &self.by_identity {
            let Identity::Property(key) = identity else {
                continue;
            };
            let id = ids[0];
            let base = PropertySchema::builtin(key).unwrap_or_default();
            // `##tag` is for any entity, and isn't about the property.
            let meta: Vec<_> = self.entities[&id]
                .meta
                .iter()
                .filter(|m| {
                    !tags::is_tag(m) && !crate::journal::is_journal(m) && !crate::ref_id::is_id(m)
                })
                .cloned()
                .collect();
            let (schema, problems) = base.with_meta(&meta);
            schemas.insert(key.clone(), schema);
            flags.insert(id, problems);
        }
        self.schemas = schemas;
        self.meta_flags = flags;
    }

    /// The schema for the property `key`.
    pub fn schema(&self, key: &str) -> Cow<'_, PropertySchema> {
        let key = normalise_name(key);
        match self.schemas.get(&key) {
            Some(schema) => Cow::Borrowed(schema),
            None => Cow::Owned(PropertySchema::builtin(&key).unwrap_or_default()),
        }
    }

    /// The entity a block is in, before duplicates are merged: the block
    /// itself if it is one. `None` for a block the index has not seen.
    pub fn entity_of(&self, block: BlockId) -> Option<BlockId> {
        self.root_of.get(&block).copied()
    }

    pub fn entity(&self, id: BlockId) -> Option<&EntityInfo> {
        self.entities.get(&id)
    }

    /// The blobs in the graph, in document order: the blocks whose first line
    /// is a `##blob` line, with what it says. A line that can't be read is
    /// not listed: the view flags it on its chip.
    pub fn assets(&self) -> impl Iterator<Item = (BlockId, crate::Asset)> + '_ {
        self.blobs.iter().cloned()
    }

    /// The parameter of every `##blob` line, as written, including the ones
    /// that can't be read, so that a damaged line can still be told from no
    /// line at all.
    pub fn blob_lines(&self) -> impl Iterator<Item = &str> {
        self.blob_lines.iter().map(String::as_str)
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
        let quoted = graph.create_block(Some(notes), None, "Quotable\n##id(abc-123)")?;
        // `id::` is an ordinary property: nothing is called by it.
        graph.create_block(Some(notes), None, "Old\nid:: old-1")?;
        let index = Index::build(&graph)?;

        let sources: Vec<_> = index.backlinks(jane).iter().map(|b| b.source).collect();
        assert_eq!(
            sources,
            [props, inline],
            "links in property values are backlinks"
        );
        assert_eq!(index.resolve_block_ref("abc-123"), Some(quoted));
        assert_eq!(index.resolve_block_ref("nope"), None);
        assert_eq!(index.resolve_block_ref("old-1"), None);
        Ok(())
    }

    /// An entity's properties are those of its first child, and only if that
    /// is a properties block: the next block in reading order is not enough
    /// (it may belong to another entity), nor is a child that merely has an
    /// inline property in its prose.
    #[test]
    fn only_a_first_child_of_properties_gives_properties() -> Result<()> {
        let graph = Graph::new();
        let prose = graph.create_block(None, None, "Prose")?;
        graph.create_block(Some(prose), None, "done, #status(done)")?;
        // Childless, and followed by an entity named like a property line.
        let empty = graph.create_block(None, None, "Empty")?;
        graph.create_block(None, None, "key:: value")?;
        let real = graph.create_entity("Real", &[("key", text("value"))])?;
        let index = Index::build(&graph)?;

        let keys = |id| -> Vec<String> {
            index
                .entity(id)
                .unwrap()
                .properties
                .iter()
                .map(|p| p.key.clone())
                .collect()
        };
        assert!(keys(prose).is_empty(), "prose is not a properties block");
        assert!(
            keys(empty).is_empty(),
            "the next entity's name is not its properties"
        );
        assert_eq!(keys(real), ["key"]);
        Ok(())
    }

    #[test]
    fn identities() -> Result<()> {
        let graph = Graph::new();
        let on = "2026-10-25".parse().unwrap();
        let day = crate::create_journal(&graph, on)?;
        let dup = crate::create_journal(&graph, on)?;
        let status = graph.create_entity("Status", &[("type", text("property"))])?;
        let broken = graph.create_entity("Not a date", &[])?;
        graph.create_block(Some(broken), None, "##journal(soon)")?;
        // `type:: journal` and `date::` mean nothing now.
        graph.create_entity(
            "Old style",
            &[("type", text("journal")), ("date", text("2026-10-25"))],
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
