//! The read side: entities, journals, favourites and references.

use std::collections::{BTreeMap, BTreeSet};

use jiff::Timestamp;
use jiff::civil::Date;
use quiddity_core::schema::{Typed, coerce};
use quiddity_core::syntax::{
    Link, MetaProperty, Property as Prop, PropertyStyle, Value, format_link, is_properties_block,
    normalise_name, parse_block, parse_properties_block,
};
use quiddity_core::{BlockId, Home, Identity, Index, Resolution, ensure_journal};
use serde::Serialize;

use crate::meta::{Inherited, Inverse};
use crate::{Error, Facts, Result, TextFacts, View};

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EntityRef {
    pub id: String,
    pub title: String,
}

/// A device that has recorded itself in the graph: one that holds it, or is
/// let in to.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    /// Its endpoint ID, which another device needs to let it in.
    pub id: String,
    /// When it last recorded itself, as an RFC 3339 timestamp in UTC.
    pub seen: String,
}

/// The entities, and which of them link to which, for drawing as a graph.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Connections {
    /// As [`View::entities`].
    pub nodes: Vec<EntityRef>,
    pub edges: Vec<Edge>,
}

/// One row of the list of every entity.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub title: String,
    /// Enough of the entity's tags to tell it from the others with the same
    /// title. Empty when the title is unique.
    pub tags: Vec<String>,
    /// Words in its blocks, not counting its title or its property lines.
    pub words: usize,
    /// Blocks in other entities that link to it.
    pub incoming: usize,
    /// When it was made, in seconds since the Unix epoch. `None` if that
    /// wasn't recorded or the history that says has been compacted away.
    pub created: Option<i64>,
    /// When a block of it was last added, moved or edited, in the same units.
    pub modified: Option<i64>,
    /// Whether it is a journal page, so the list can leave the days out.
    pub journal: bool,
}

/// Where the UI opens: the graph's `home` setting, with an entity found.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Start {
    Journals,
    All,
    Graph,
    Entity { id: String },
}

/// One entity linking to another, from anywhere in its blocks. However many
/// links there are, there is one edge.
#[derive(Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub id: String,
    pub text: String,
    /// The properties written in `text`, in order, for rendering as chips.
    pub properties: Vec<Property>,
    pub children: Vec<Block>,
}

/// One property written in a block's text.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Property {
    pub key: String,
    pub value: Option<PropertyValue>,
    /// `#key(value)` within the text, rather than `key:: value` on a line.
    pub inline: bool,
    /// Where it is written, in UTF-16 units of `text`, which is what
    /// JavaScript counts in. For a line property, the whole line.
    pub start: usize,
    pub end: usize,
    /// The entity that defines the property, if there is one.
    pub definition: Option<String>,
    /// A `##key(parameter)` line, whose parameter is its value as text.
    pub meta: bool,
    /// What doesn't fit the property's schema here, in words. Only the
    /// properties an entity writes for itself, and its meta-properties, are
    /// checked.
    pub flags: Vec<String>,
}

/// A property's value, as written.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PropertyValue {
    Text {
        text: String,
    },
    /// `key` is how the link is written between its brackets, to look it up in
    /// the `links` of the entity or reference it is in.
    Link {
        name: String,
        key: String,
    },
    Ref {
        id: String,
    },
    List {
        items: Vec<PropertyValue>,
    },
}

/// Records where a link goes, under its [`link_key`].
pub(crate) fn add_link(index: &Index, link: &Link, links: &mut BTreeMap<String, ResolvedLink>) {
    let target = match index.resolve(link) {
        Resolution::Unique(id) => LinkTarget::Entity {
            id: index.canonical(id).to_string(),
        },
        Resolution::Ambiguous(ids) => LinkTarget::Ambiguous {
            ids: ids.iter().map(ToString::to_string).collect(),
        },
        Resolution::Missing => LinkTarget::Missing,
    };
    links.insert(
        link_key(link),
        ResolvedLink {
            name: link.name.clone(),
            qualifiers: link_qualifiers(link),
            target,
        },
    );
}

/// A link as written between its brackets: the key of [`Entity::links`].
pub(crate) fn link_key(link: &Link) -> String {
    let written = format_link(link);
    written
        .strip_prefix("[[")
        .and_then(|s| s.strip_suffix("]]"))
        .unwrap_or(&written)
        .to_owned()
}

/// The qualifiers of a link as written, which follow its name in [`link_key`].
pub(crate) fn link_qualifiers(link: &Link) -> String {
    let bare = Link {
        name: link.name.clone(),
        qualifiers: Vec::new(),
    };
    link_key(link)[link_key(&bare).len()..].to_owned()
}

pub(crate) fn property_value(value: &Value) -> PropertyValue {
    match value {
        Value::Text(text) => PropertyValue::Text { text: text.clone() },
        Value::Link(link) => PropertyValue::Link {
            name: link.name.clone(),
            key: link_key(link),
        },
        Value::BlockRef(id) => PropertyValue::Ref { id: id.clone() },
        Value::List(items) => PropertyValue::List {
            items: items.iter().map(property_value).collect(),
        },
    }
}

/// UTF-16 units in the first `byte` bytes of `text`.
fn utf16_at(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

fn property(text: &str, p: &Prop, index: &Index) -> Property {
    // A property's key goes to the entity that defines it, and a tag to the
    // entity it names.
    let definition = if p.style == PropertyStyle::Inline {
        match index.resolve(&quiddity_core::tags::link(&p.key)) {
            Resolution::Unique(id) => Some(index.canonical(id)),
            _ => None,
        }
    } else {
        index.property_entity(&p.key).map(|id| index.canonical(id))
    };
    Property {
        definition: definition.map(|id| id.to_string()),
        key: p.key.clone(),
        value: p.value.as_ref().map(property_value),
        inline: p.style == PropertyStyle::Inline,
        start: utf16_at(text, p.span.start),
        end: utf16_at(text, p.span.end),
        meta: false,
        flags: Vec::new(),
    }
}

fn meta_property(text: &str, m: &MetaProperty) -> Property {
    Property {
        definition: None,
        key: m.key.clone(),
        value: match (&m.value, &m.parameter) {
            (Some(v), _) => Some(property_value(v)),
            (None, p) => p.as_ref().map(|p| PropertyValue::Text { text: p.clone() }),
        },
        inline: false,
        start: utf16_at(text, m.span.start),
        end: utf16_at(text, m.span.end),
        meta: true,
        flags: Vec::new(),
    }
}

/// Where a `[[link]]` goes.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LinkTarget {
    Entity { id: String },
    Ambiguous { ids: Vec<String> },
    Missing,
}

/// A link in a block, as the UI draws it.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedLink {
    /// What the link is called, which is all that is shown of it.
    pub name: String,
    /// The `#key(value)` qualifiers that narrow it, written out; empty if
    /// there are none. Shown on hover rather than in the text.
    pub qualifiers: String,
    #[serde(flatten)]
    pub target: LinkTarget,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Entity {
    pub id: String,
    pub title: String,
    /// False for journals and property entities, which are identified by
    /// their date or name.
    pub renamable: bool,
    pub blocks: Vec<Block>,
    /// Every link in the entity's blocks, and in its inherited values,
    /// keyed by the text between the brackets as the syntax crate writes it.
    /// Links written some other way are not found here and render as
    /// unresolved.
    pub links: BTreeMap<String, ResolvedLink>,
    /// Values it has from other entities, through a property with
    /// `##inherits` or `##transitive`.
    pub inherited: Vec<Inherited>,
    /// For each inverse label that has any, the entities related to it the
    /// other way round, as a tree.
    pub inverses: Vec<Inverse>,
    /// What doesn't fit a schema and has no chip of its own to show on, in
    /// words.
    pub flags: Vec<String>,
    /// The binary this entity describes, if its `##asset` line can be read.
    pub asset: Option<AssetInfo>,
}

/// A binary kept in the graph: what its entity says about it.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AssetInfo {
    /// The BLAKE3 hash of the bytes, in hex: what to ask the shell for.
    pub hash: String,
    pub mime: String,
    /// In bytes.
    pub size: u64,
    pub name: Option<String>,
}

/// The blocks of one entity that mention another.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub source: EntityRef,
    /// Each with its children, for context.
    pub blocks: Vec<Block>,
    /// As [`Entity::links`], for the links in these blocks.
    pub links: BTreeMap<String, ResolvedLink>,
}

impl View {
    pub(crate) fn entity_id(&self, id: &str) -> Result<BlockId> {
        let bad = || Error::NotAnEntity(id.to_owned());
        let id: BlockId = id.parse().map_err(|_| bad())?;
        if !self.graph.contains(id) || self.graph.parent(id).is_some() {
            return Err(bad());
        }
        Ok(self.index()?.canonical(id))
    }

    pub(crate) fn entity_ref(&self, id: BlockId) -> Result<EntityRef> {
        Ok(EntityRef {
            id: id.to_string(),
            title: self.graph.entity_name(id)?,
        })
    }

    /// Entities that are not merged into another, by name.
    pub fn entities(&self) -> Result<Vec<EntityRef>> {
        let index = self.index()?;
        let mut refs: Vec<_> = index
            .entities()
            .map(|e| EntityRef {
                id: e.id.to_string(),
                title: e.name.clone(),
            })
            .collect();
        refs.sort_by_cached_key(|e| (e.title.to_lowercase(), e.id.clone()));
        Ok(refs)
    }

    /// Every entity with what is needed to choose between them, in the order
    /// of [`View::entities`].
    pub fn summaries(&self) -> Result<Vec<Summary>> {
        self.refresh_activity()?;
        let activity = self.activity.borrow();
        let mut modified: BTreeMap<BlockId, Timestamp> = BTreeMap::new();
        let mut words: BTreeMap<BlockId, usize> = BTreeMap::new();
        let mut tags: BTreeMap<BlockId, BTreeSet<String>> = BTreeMap::new();
        let index = self.index()?;
        let corpus = self.corpus()?;
        let mut facts = self.facts.borrow_mut();
        let Facts {
            text: texts,
            placed,
        } = &mut *facts;
        let placed = placed.get_or_insert_with(|| {
            self.count(|w| w.placements_read += 1);
            corpus
                .iter()
                .filter_map(|b| Some((b.id, self.graph.placed_at(b.id)?)))
                .collect()
        });
        let mut current = None;
        for scanned in corpus.iter() {
            let block = scanned.id;
            if scanned.parent.is_none() {
                current = Some(index.canonical(block));
                // The tags of its properties block, in either form.
                if let (Some(entity), Some(info)) = (current, index.entity(block)) {
                    tags.entry(entity).or_default().extend(
                        info.properties
                            .iter()
                            .filter(|p| p.style == PropertyStyle::Inline && p.value.is_none())
                            .map(|p| normalise_name(&p.key)),
                    );
                }
            }
            let Some(entity) = current else {
                continue;
            };
            let touched = [
                placed.get(&block).copied(),
                activity.edited.get(&block).copied(),
            ];
            if let Some(at) = touched.into_iter().flatten().max() {
                let latest = modified.entry(entity).or_insert(at);
                *latest = (*latest).max(at);
            }
            if scanned.parent.is_none() {
                continue;
            }
            let text = texts.entry(block).or_insert_with(|| {
                self.count(|w| w.blocks_summed += 1);
                TextFacts {
                    words: if is_properties_block(&scanned.text) {
                        0
                    } else {
                        scanned.text.split_whitespace().count()
                    },
                    tags: tags_in(&scanned.text),
                }
            });
            if text.words > 0 {
                *words.entry(entity).or_default() += text.words;
            }
            if !text.tags.is_empty() {
                tags.entry(entity)
                    .or_default()
                    .extend(text.tags.iter().cloned());
            }
        }
        drop(facts);
        drop(corpus);
        let rows = self
            .entities()?
            .into_iter()
            .map(|e| Ok((self.entity_id(&e.id)?, e)))
            .collect::<Result<Vec<_>>>()?;
        let mut same_title: BTreeMap<String, Vec<BlockId>> = BTreeMap::new();
        for (id, e) in &rows {
            same_title
                .entry(e.title.to_lowercase())
                .or_default()
                .push(*id);
        }
        let none = BTreeSet::new();
        let tags_of = |id: &BlockId| tags.get(id).unwrap_or(&none);
        let mut summaries = Vec::new();
        for (id, e) in &rows {
            let rivals: Vec<_> = same_title[&e.title.to_lowercase()]
                .iter()
                .filter(|j| *j != id)
                .map(tags_of)
                .collect();
            let incoming = index
                .backlinks(*id)
                .iter()
                .filter(|b| index.entity_of(b.source).map(|e| index.canonical(e)) != Some(*id))
                .count();
            summaries.push(Summary {
                id: e.id.clone(),
                title: e.title.clone(),
                tags: distinguishing(tags_of(id), &rivals),
                words: words.get(id).copied().unwrap_or(0),
                incoming,
                created: self.graph.created_at(*id).map(Timestamp::as_second),
                modified: modified.get(id).copied().map(Timestamp::as_second),
                journal: matches!(
                    index.entity(*id).and_then(|e| e.identity()),
                    Some(Identity::Journal(_))
                ),
            });
        }
        Ok(summaries)
    }

    /// Every entity, and the links between them. Links to nowhere, and an
    /// entity's links to itself, are left out.
    pub fn connections(&self) -> Result<Connections> {
        let mut edges = BTreeSet::new();
        {
            let index = self.index()?;
            for target in index.entities() {
                for backlink in index.backlinks(target.id) {
                    let Some(source) = index.entity_of(backlink.source).map(|e| index.canonical(e))
                    else {
                        continue;
                    };
                    if source != target.id {
                        edges.insert(Edge {
                            source: source.to_string(),
                            target: target.id.to_string(),
                        });
                    }
                }
            }
        }
        Ok(Connections {
            nodes: self.entities()?,
            edges: edges.into_iter().collect(),
        })
    }

    /// The devices the graph lists, most recently seen first. They are sync
    /// metadata, not entities: the graph keeps them beside the blocks.
    pub fn devices(&self) -> Vec<DeviceInfo> {
        let mut devices = self.graph.devices();
        devices.sort_by(|a, b| b.seen.cmp(&a.seen).then_with(|| a.id.cmp(&b.id)));
        devices
            .into_iter()
            .map(|d| DeviceInfo {
                id: d.id,
                seen: d.seen.to_string(),
            })
            .collect()
    }

    /// Entities whose `favourite` property is true.
    pub fn favourites(&self) -> Result<Vec<EntityRef>> {
        let index = self.index()?;
        let schema = index.schema("favourite");
        let mut favourites = Vec::new();
        for entity in index.entities() {
            let favourite = entity
                .values("favourite")
                .any(|v| matches!(coerce(v, &schema), Ok(Typed::Boolean(true))));
            if favourite && index.canonical(entity.id) == entity.id {
                favourites.push(self.entity_ref(entity.id)?);
            }
        }
        favourites.sort_by_cached_key(|e| (e.title.to_lowercase(), e.id.clone()));
        Ok(favourites)
    }

    /// Where the UI opens, as the graph's settings say. A link that finds no
    /// entity, or several, opens the journals instead: there is nowhere
    /// better to go, and the journals are always there.
    pub fn start(&self) -> Result<Start> {
        Ok(match self.graph.settings().home.unwrap_or(Home::Journals) {
            Home::Journals => Start::Journals,
            Home::All => Start::All,
            Home::Graph => Start::Graph,
            Home::Entity(link) => {
                let index = self.index()?;
                match index.resolve(&link) {
                    Resolution::Unique(id) => Start::Entity {
                        id: index.canonical(id).to_string(),
                    },
                    _ => Start::Journals,
                }
            }
        })
    }

    /// Journal pages from `today` back, newest first.
    pub fn journals(&self, today: Date) -> Result<Vec<EntityRef>> {
        let index = self.index()?;
        let mut days: Vec<_> = index
            .entities()
            .filter_map(|e| match e.identity() {
                Some(Identity::Journal(date)) if date <= today => Some((date, e)),
                _ => None,
            })
            .collect();
        days.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.id.cmp(&a.1.id)));
        Ok(days
            .into_iter()
            .map(|(_, e)| EntityRef {
                id: e.id.to_string(),
                title: e.name.clone(),
            })
            .collect())
    }

    /// The journal page for `today`, created if there isn't one.
    pub fn ensure_journal(&self, today: Date) -> Result<EntityRef> {
        let existing = self.index()?.journal(today);
        let id = match existing {
            Some(id) => id,
            None => {
                let id = ensure_journal(&self.graph, &*self.index()?, today)?;
                self.graph.commit();
                self.changed();
                id
            }
        };
        let id = self.index()?.canonical(id);
        self.entity_ref(id)
    }

    pub fn entity(&self, id: &str) -> Result<Entity> {
        let id = self.entity_id(id)?;
        let mut links = BTreeMap::new();
        let blocks = self
            .graph
            .children(id)
            .into_iter()
            .map(|child| self.block(child, &mut links))
            .collect::<Result<_>>()?;
        let mut blocks: Vec<Block> = blocks;
        let renamable = self
            .index()?
            .entity(id)
            .is_some_and(|e| e.identity().is_none());
        let props_block = self.graph.properties_block(id)?.map(|b| b.to_string());
        let chips = blocks
            .first_mut()
            .filter(|b| Some(&b.id) == props_block.as_ref())
            .map(|b| &mut b.properties);
        let flags = self.flag(id, chips)?;
        let inherited = self.inherited(id, &mut links)?;
        let inverses = self.inverses(id)?;
        let asset = self
            .index()?
            .entity(id)
            .and_then(|e| e.asset()?.ok())
            .map(|a| AssetInfo {
                hash: a.hash,
                mime: a.mime,
                size: a.size,
                name: a.name,
            });
        Ok(Entity {
            id: id.to_string(),
            title: self.graph.entity_name(id)?,
            renamable,
            blocks,
            links,
            inherited,
            inverses,
            flags,
            asset,
        })
    }

    fn block(&self, id: BlockId, links: &mut BTreeMap<String, ResolvedLink>) -> Result<Block> {
        let text = self.graph.text(id)?;
        // A properties block is its entity's first child, if it starts like
        // one; only its property, tag and `##` lines are read.
        let is_first_child = self.graph.parent(id).is_some_and(|p| {
            self.graph.parent(p).is_none() && self.graph.children(p).first() == Some(&id)
        });
        let props_block = if is_first_child {
            parse_properties_block(&text)
        } else {
            None
        };
        let here = props_block.is_some();
        let parsed = match props_block {
            Some(found) => found.block,
            None => parse_block(&text),
        };
        let index = self.index()?;
        for link in &parsed.links {
            add_link(&index, link, links);
        }
        let mut properties: Vec<Property> = parsed
            .properties
            .iter()
            .map(|p| {
                let mut chip = property(&text, p, &index);
                // A property says something about an entity, so it goes in the
                // entity's properties block. `id` is the exception until
                // block references have a `##` property of their own.
                if !here && p.style == PropertyStyle::Line && normalise_name(&p.key) != "id" {
                    chip.flags
                        .push("Has no effect outside an entity's properties block".into());
                }
                chip
            })
            .collect();
        drop(index);
        if !parsed.meta.is_empty() {
            properties.extend(parsed.meta.iter().map(|m| {
                let mut chip = meta_property(&text, m);
                if !here {
                    chip.flags
                        .push("Has no effect outside a property's properties block".into());
                }
                chip
            }));
            properties.sort_by_key(|p| p.start);
        }
        let children = self
            .graph
            .children(id)
            .into_iter()
            .map(|child| self.block(child, links))
            .collect::<Result<_>>()?;
        Ok(Block {
            id: id.to_string(),
            text,
            properties,
            children,
        })
    }

    /// Blocks that link to the entity, by the entity they are in.
    pub fn references(&self, id: &str) -> Result<Vec<Reference>> {
        let target = self.entity_id(id)?;
        let sources = self
            .index()?
            .backlinks(target)
            .iter()
            .map(|b| b.source)
            .collect();
        self.grouped(target, sources)
    }

    /// Blocks that mention the entity's name or an alias in plain text,
    /// without linking to it.
    pub fn unlinked_references(&self, id: &str) -> Result<Vec<Reference>> {
        let target = self.entity_id(id)?;
        let (names, linked) = {
            let index = self.index()?;
            let Some(info) = index.entity(target) else {
                return Ok(Vec::new());
            };
            if matches!(info.identity(), Some(Identity::Journal(_))) {
                return Ok(Vec::new());
            }
            let names: Vec<String> = std::iter::once(info.name.clone())
                .chain(
                    info.values("alias")
                        .flat_map(|v| match v {
                            Value::List(items) => items.iter().collect(),
                            v => vec![v],
                        })
                        .filter_map(|v| match v {
                            Value::Text(s) => Some(s.clone()),
                            Value::Link(link) => Some(link.name.clone()),
                            _ => None,
                        }),
                )
                .map(|n| n.trim().to_lowercase())
                .filter(|n| !n.is_empty())
                .collect();
            let linked: BTreeSet<_> = index.backlinks(target).iter().map(|b| b.source).collect();
            (names, linked)
        };
        let mut found = BTreeSet::new();
        for block in self.graph.blocks() {
            // Entity names aren't prose, and property values are structured.
            if self.graph.parent(block).is_none() || linked.contains(&block) {
                continue;
            }
            let text = self.graph.text(block)?;
            if is_properties_block(&text) {
                continue;
            }
            let prose = without_links(&text).to_lowercase();
            if names.iter().any(|n| contains_word(&prose, n)) {
                found.insert(block);
            }
        }
        self.grouped(target, found)
    }

    /// Groups blocks by entity, leaving out the target's own and any block
    /// already shown inside another.
    fn grouped(&self, target: BlockId, blocks: BTreeSet<BlockId>) -> Result<Vec<Reference>> {
        let mut by_entity: BTreeMap<BlockId, Vec<BlockId>> = BTreeMap::new();
        for &block in &blocks {
            let root = self.graph.entity_of(block);
            let entity = self.index()?.canonical(root);
            let nested = std::iter::successors(self.graph.parent(block), |&p| self.graph.parent(p))
                .any(|ancestor| blocks.contains(&ancestor));
            if entity != target && block != root && !nested {
                by_entity.entry(entity).or_default().push(block);
            }
        }
        let mut groups = Vec::new();
        for (entity, blocks) in by_entity {
            let mut links = BTreeMap::new();
            let blocks = blocks
                .into_iter()
                .map(|b| self.block(b, &mut links))
                .collect::<Result<_>>()?;
            let date = match self.index()?.entity(entity).and_then(|e| e.identity()) {
                Some(Identity::Journal(date)) => Some(date),
                _ => None,
            };
            let source = self.entity_ref(entity)?;
            groups.push((
                date,
                Reference {
                    source,
                    blocks,
                    links,
                },
            ));
        }
        // Recent days first, then everything else by name.
        groups.sort_by_cached_key(|(date, g)| {
            (
                date.is_none(),
                std::cmp::Reverse(*date),
                g.source.title.to_lowercase(),
                g.source.id.clone(),
            )
        });
        Ok(groups.into_iter().map(|(_, g)| g).collect())
    }
}

/// `text` with `[[links]]` and `((block refs))` blanked out, so a name inside
/// one doesn't count as a plain mention.
pub(crate) fn without_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let skipped = ["[[", "(("].iter().find_map(|open| {
            let close = if *open == "[[" { "]]" } else { "))" };
            let inner = rest.strip_prefix(open)?;
            inner.find(close).map(|end| &inner[end + 2..])
        });
        match skipped {
            Some(after) => {
                out.push(' ');
                rest = after;
            }
            None => {
                let c = rest.chars().next().expect("rest is not empty");
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    out
}

/// Whether `name` occurs in `text` as whole words.
pub(crate) fn contains_word(text: &str, name: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(name).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + name.len()..].chars().next();
        !before.is_some_and(word) && !after.is_some_and(word)
    })
}

/// The tags written in a block's text, as `#tag`. A tag in a properties block's
/// `##tag` line is read from the index instead, which knows which block that
/// is.
fn tags_in(text: &str) -> Vec<String> {
    parse_block(text)
        .properties
        .iter()
        .filter(|p| p.style == PropertyStyle::Inline)
        .map(|p| normalise_name(&p.key))
        .collect()
}

/// Some of `own` that tell it from every one of `rivals`: each rival lacks at
/// least one of them. Greedy, taking the tag that rules out most rivals first
/// (ties to the alphabetically first). If a rival has all of `own`, nothing
/// can tell them apart, and it is left out of the reckoning.
pub(crate) fn distinguishing(own: &BTreeSet<String>, rivals: &[&BTreeSet<String>]) -> Vec<String> {
    let mut remaining: Vec<_> = rivals.to_vec();
    let mut chosen = Vec::new();
    while !remaining.is_empty() {
        let best = own
            .iter()
            .filter(|t| !chosen.contains(*t))
            .map(|t| (remaining.iter().filter(|r| !r.contains(t)).count(), t))
            .filter(|(n, _)| *n > 0)
            .max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(a.1)));
        let Some((_, tag)) = best else { break };
        remaining.retain(|r| r.contains(tag));
        chosen.push(tag.clone());
    }
    chosen.sort();
    chosen
}
