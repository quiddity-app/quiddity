use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use jiff::civil::Date;
use quiddity_syntax::{
    Block, Link, MetaProperty, Property, PropertyStyle, Qualifier, Value, normalise_name,
    parse_block, parse_properties_block, ref_items,
};

use crate::effective::Effective;
use crate::schema::{PropertySchema, ValueType};
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

/// A block that links to a name no entity has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    pub source: BlockId,
    /// The name as that block wrote it, trimmed. The first in document order
    /// is the one an entity made for the name would take.
    pub name: String,
}

/// An entity with a property whose key no entity has as a name: the property
/// has no entity to define it, and so a virtual one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyUse {
    pub entity: BlockId,
    /// The key as that entity wrote it.
    pub key: String,
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
    #[must_use]
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
            .flat_map(ref_items)
            .map(|v| canonical_value(&v))
            .collect()
    }

    /// For entities fully identified by their properties, that identity.
    pub fn identity(&self) -> Option<Identity> {
        crate::journal::date(&self.meta).map(Identity::Journal)
    }

    /// Whether one of the entity's properties satisfies the qualifier.
    #[must_use]
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
    /// Names that are linked to and that no entity has, by normalised name.
    mentions: BTreeMap<String, Vec<Mention>>,
    /// Property keys that no entity has as a name, by normalised key, with the
    /// entities that use each in document order. Read from properties blocks,
    /// which `update_text` leaves to a rebuild, so it never has to maintain it.
    undefined_keys: BTreeMap<String, Vec<KeyUse>>,
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
    /// For each property with `##through` and entity, the entities that
    /// have it as a value of the property it goes through, as written: the
    /// subclasses of a class, say. See [`Index::through_children`].
    pub(crate) through: HashMap<(String, BlockId), Vec<BlockId>>,
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

        index.read_undefined_keys(blocks);
        // Before the links are read: which bare text is a link depends on
        // each property's type.
        index.read_schemas();

        let mut references = Vec::new();
        for (at, scanned) in blocks.iter().enumerate() {
            if scanned.parent.is_none() {
                continue;
            }
            let block = scanned.id;
            // An entity's first child comes straight after it, and if it is a
            // properties block only its own lines are read.
            let owner = at
                .checked_sub(1)
                .and_then(|before| blocks.get(before))
                .filter(|before| before.parent.is_none())
                .map(|before| before.id);
            let first_child = owner.is_some();
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
            // Only a properties block's lines are the entity's properties, so
            // for any other first child this finds nothing.
            if let Some(entity) = owner {
                references.extend(
                    index
                        .bare_references(entity)
                        .into_iter()
                        .map(|link| (block, link)),
                );
            }
        }
        let references_for_mentions = references.clone();
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
        for &(source, ref link) in &references_for_mentions {
            index.add_mention(source, link, None);
        }
        index.compute_effective();
        index
    }

    /// The links an entity's properties make without brackets: the names in
    /// the text of a `ref` property, which mean what `[[name]]` would. They
    /// are links like any other, so they are backlinks and mentions too.
    fn bare_references(&self, entity: BlockId) -> Vec<Link> {
        let Some(info) = self.entities.get(&entity) else {
            return Vec::new();
        };
        let mut links = Vec::new();
        for property in &info.properties {
            let Some(value) = &property.value else {
                continue;
            };
            if self.schema(&property.key).value_type != ValueType::Ref {
                continue;
            }
            for item in ref_items(value) {
                if let Value::Text(name) = item
                    && !name.trim().is_empty()
                {
                    links.push(Link {
                        name,
                        qualifiers: Vec::new(),
                    });
                }
            }
        }
        links
    }

    /// Notes each property key, written as a line in an entity's properties
    /// block, that no entity is named for, in document order. A tag is a link
    /// and a `##` line is a meta-property: neither is a key.
    fn read_undefined_keys(&mut self, blocks: &[Scanned]) {
        for block in blocks.iter().filter(|b| b.parent.is_none()) {
            if self.canonical.contains_key(&block.id) {
                continue;
            }
            let info = &self.entities[&block.id];
            for property in info
                .properties
                .iter()
                .filter(|p| p.style == PropertyStyle::Line)
            {
                let key = normalise_name(&property.key);
                if key.is_empty() || self.by_name.contains_key(&key) {
                    continue;
                }
                let uses = self.undefined_keys.entry(key).or_default();
                if uses.last().is_none_or(|u| u.entity != block.id) {
                    uses.push(KeyUse {
                        entity: block.id,
                        key: property.key.trim().to_owned(),
                    });
                }
            }
        }
    }

    /// The property keys that no entity has as a name, with who uses each, in
    /// document order. Like [`Index::mentions`], they are entities in all but
    /// being written: see `quiddity-view`.
    pub fn undefined_keys(&self) -> impl Iterator<Item = (&str, &[KeyUse])> {
        self.undefined_keys
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    /// Who uses the property `key`, if no entity is named for it.
    pub fn undefined_key(&self, key: &str) -> &[KeyUse] {
        self.undefined_keys
            .get(&normalise_name(key))
            .map_or(&[], Vec::as_slice)
    }

    /// Notes a link to a name nobody has made, if `link` is one: no qualifiers,
    /// since qualifiers have to be written on an entity of their own, and no
    /// entity of that name. `at` is where the block is in document order, when
    /// the index is being kept up to date and so the list has to stay in it.
    fn add_mention(&mut self, source: BlockId, link: &Link, at: Option<usize>) {
        if !link.qualifiers.is_empty() || !matches!(self.resolve(link), Resolution::Missing) {
            return;
        }
        let list = self.mentions.entry(normalise_name(&link.name)).or_default();
        let mention = Mention {
            source,
            name: link.name.trim().to_owned(),
        };
        match at {
            None => list.push(mention),
            Some(at) => {
                let position = &self.position;
                let after = list.partition_point(|m| position[&m.source] <= at);
                list.insert(after, mention);
            }
        }
    }

    /// The names that are linked to and that no entity has, with where each is
    /// linked from, in document order. They are entities in all but being
    /// written: see `quiddity-view`.
    pub fn mentions(&self) -> impl Iterator<Item = (&str, &[Mention])> {
        self.mentions
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    /// Where the name `name` is linked from, if no entity has it.
    pub fn mention(&self, name: &str) -> &[Mention] {
        self.mentions
            .get(&normalise_name(name))
            .map_or(&[], Vec::as_slice)
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
        // A block is only ever in the mentions of the names its links write.
        for link in &old.links {
            let name = normalise_name(&link.name);
            if let Some(list) = self.mentions.get_mut(&name) {
                list.retain(|m| m.source != block);
                if list.is_empty() {
                    self.mentions.remove(&name);
                }
            }
        }
        for link in &new.links {
            self.add_mention(block, link, Some(at));
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
    /// schemas. An entity defines the property of its name, and of each alias,
    /// when it is the only one with that name ([`Index::property_entity`]).
    fn read_schemas(&mut self) {
        let mut schemas = HashMap::new();
        let mut flags = HashMap::new();
        for (&id, info) in &self.entities {
            // `##tag` is for any entity, and isn't about the property.
            let meta: Vec<_> = info
                .meta
                .iter()
                .filter(|m| {
                    !tags::is_tag(m) && !crate::journal::is_journal(m) && !crate::ref_id::is_id(m)
                })
                .cloned()
                .collect();
            if meta.is_empty() || !self.defines_property(id) {
                continue;
            }
            let names = std::iter::once(normalise_name(&info.name))
                .chain(info.aliases().into_iter().map(canonical_value));
            for key in names.filter(|k| !k.is_empty()) {
                let base = PropertySchema::builtin(&key).unwrap_or_default();
                let (schema, problems) = base.with_meta(&meta);
                if key == normalise_name(&info.name) {
                    flags.insert(id, problems);
                }
                schemas.insert(key, schema);
            }
        }
        self.schemas = schemas;
        self.meta_flags = flags;
    }

    /// The schema for the property `key`.
    #[must_use]
    pub fn schema(&self, key: &str) -> Cow<'_, PropertySchema> {
        let key = normalise_name(key);
        match self.schemas.get(&key) {
            Some(schema) => Cow::Borrowed(schema),
            None => Cow::Owned(PropertySchema::builtin(&key).unwrap_or_default()),
        }
    }

    /// The entity a block is in, before duplicates are merged: the block
    /// itself if it is one. `None` for a block the index has not seen.
    #[must_use]
    pub fn entity_of(&self, block: BlockId) -> Option<BlockId> {
        self.root_of.get(&block).copied()
    }

    #[must_use]
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
    #[must_use]
    pub fn canonical(&self, id: BlockId) -> BlockId {
        self.canonical.get(&id).copied().unwrap_or(id)
    }

    /// Entities with this name or alias.
    #[must_use]
    pub fn named(&self, name: &str) -> Vec<BlockId> {
        self.by_name
            .get(&normalise_name(name))
            .map(|ids| ids.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Follows a link: entities with the link's name or alias whose
    /// properties match all of the link's qualifiers.
    #[must_use]
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
    #[must_use]
    pub fn resolve_block_ref(&self, id: &str) -> Option<BlockId> {
        self.by_ref_id.get(id).copied()
    }

    /// Blocks linking to `target`.
    pub fn backlinks(&self, target: BlockId) -> &[Backlink] {
        self.backlinks.get(&target).map_or(&[], Vec::as_slice)
    }

    /// The entity with this identity.
    #[must_use]
    pub fn by_identity(&self, identity: &Identity) -> Option<BlockId> {
        self.by_identity.get(identity).map(|ids| ids[0])
    }

    /// The journal page for `date`.
    #[must_use]
    pub fn journal(&self, date: Date) -> Option<BlockId> {
        self.by_identity(&Identity::Journal(date))
    }

    /// The day a link names, if it leads to a journal.
    #[must_use]
    pub fn link_day(&self, link: &Link) -> Option<Date> {
        let Resolution::Unique(id) = self.resolve(link) else {
            return None;
        };
        match self.entity(self.canonical(id))?.identity()? {
            Identity::Journal(day) => Some(day),
        }
    }

    /// A date property's value with each link to a journal read as its day,
    /// so that `[[2026-10-25]]` and `2026-10-25` mean the same. Anything else
    /// is left as it was.
    #[must_use]
    pub fn read_days(&self, value: &Value) -> Value {
        match value {
            Value::Link(link) => self
                .link_day(link)
                .map_or_else(|| value.clone(), |day| Value::Text(day.to_string())),
            Value::List(items) => Value::List(items.iter().map(|v| self.read_days(v)).collect()),
            _ => value.clone(),
        }
    }

    /// The entity that defines the property `key`: the only entity with that
    /// name or alias, found as a link to the name alone would. With none, or
    /// several, the property has no entity to define it.
    #[must_use]
    pub fn property_entity(&self, key: &str) -> Option<BlockId> {
        let found = self.named(key);
        match found.as_slice() {
            [only] => Some(*only),
            _ => None,
        }
    }

    /// Whether `id` is the entity that defines the property of its own name.
    #[must_use]
    pub fn defines_property(&self, id: BlockId) -> bool {
        self.entities
            .get(&id)
            .is_some_and(|info| self.property_entity(&info.name) == Some(id))
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

    /// A link to a journal is its day, and nothing else is.
    #[test]
    fn a_link_to_a_journal_reads_as_its_day() -> Result<()> {
        let graph = Graph::new();
        let on: Date = "2026-10-25".parse().unwrap();
        crate::create_journal(&graph, on)?;
        graph.create_entity("Elsewhere", &[])?;
        let index = Index::build(&graph)?;
        let link = |name: &str| Link {
            name: name.to_owned(),
            qualifiers: Vec::new(),
        };

        assert_eq!(index.link_day(&link("2026-10-25")), Some(on));
        assert_eq!(index.link_day(&link("Elsewhere")), None);
        assert_eq!(index.link_day(&link("2026-10-26")), None, "no journal");
        let read = |v: Value| index.read_days(&v);
        assert_eq!(
            read(Value::List(vec![
                Value::Link(link("2026-10-25")),
                Value::Link(link("Elsewhere")),
                text_value("2026-10-27"),
            ])),
            Value::List(vec![
                text_value("2026-10-25"),
                Value::Link(link("Elsewhere")),
                text_value("2026-10-27"),
            ])
        );
        Ok(())
    }

    fn text_value(s: &str) -> Value {
        Value::Text(s.to_owned())
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
