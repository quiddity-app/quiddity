//! Numbers about the graph, and a check that what is derived from it agrees
//! with it: the page for finding out why Quiddity is showing what it is.
//!
//! [`View::stats`] is cheap, a walk over what the index already holds.
//! [`View::check`] is not: it reads the graph again and builds a second index
//! to compare with the one in use, so it is asked for separately.

use std::collections::{BTreeMap, BTreeSet};

use quiddity_core::syntax::normalise_name;
use quiddity_core::{BlockId, Identity, Index, Scanned};
use serde::Serialize;

use crate::virtual_entity::{self, Virtual};
use crate::{Result, View};

/// What the graph holds, in numbers.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    /// Every block, entities' root blocks included.
    pub blocks: usize,
    /// Entities that have been written, merged duplicates not counted.
    pub entities: usize,
    /// How many of them are journal pages.
    pub journals: usize,
    /// Entities that were made twice (a journal on two devices, say) and have
    /// been merged into another. They are kept, and hidden.
    pub merged_duplicates: usize,
    /// Blocks that hold a file.
    pub blobs: usize,
    /// Links to entities that have been written, from any block (a link to
    /// itself included), once for each link.
    pub links: usize,
    /// Links to a name nobody has made: the sum of the virtual entities'
    /// `links`.
    pub links_to_nowhere: usize,
    /// Blocks with a link that matches more than one entity.
    pub ambiguous: usize,
    /// Entities that no other links to and that link to nothing, journal
    /// pages left out since each is one.
    pub orphans: usize,
    /// The names that are linked to or are a property's key and that no
    /// entity has, most linked first.
    pub virtuals: Vec<VirtualStat>,
}

/// One virtual entity, and how much it is wanted.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VirtualStat {
    /// Its ID: `~` and the name as names are compared.
    pub id: String,
    pub title: String,
    /// Links to it, once for each, so a block that links twice counts twice.
    pub links: usize,
    /// The entities those links are in, once each.
    pub linked_from: usize,
    /// Entities that use the name as a property key.
    pub key_uses: usize,
}

/// What [`View::check`] found.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    /// Whether what is derived from the graph agrees with it. A problem here
    /// is a bug, in Quiddity or in the way it was kept up to date.
    pub integrity: Vec<Outcome>,
    /// Whether what is written fits what it says about itself. A problem here
    /// is in the text, and is what the page of the entity flags.
    pub content: Vec<Outcome>,
}

impl Check {
    /// How many problems there are in the integrity checks.
    #[must_use]
    pub fn faults(&self) -> usize {
        self.integrity.iter().map(|o| o.problems.len()).sum()
    }
}

/// One thing checked, and what was wrong with it.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// What was checked, as a claim that is true if there are no problems.
    pub name: String,
    pub problems: Vec<Problem>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub message: String,
    /// The entity it is about, if there is one to go to.
    pub id: Option<String>,
}

impl Problem {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            id: None,
        }
    }

    fn about(id: impl ToString, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            id: Some(id.to_string()),
        }
    }
}

/// How many problems one check lists before saying how many it left out, so a
/// graph that is wrong everywhere doesn't send the page a thousand lines.
pub(crate) const LISTED: usize = 50;

pub(crate) fn outcome(name: &str, mut problems: Vec<Problem>) -> Outcome {
    if problems.len() > LISTED {
        let more = problems.len() - LISTED;
        problems.truncate(LISTED);
        problems.push(Problem::new(format!("…and {more} more")));
    }
    Outcome {
        name: name.to_owned(),
        problems,
    }
}

impl View {
    /// The graph in numbers, and every virtual entity with its incoming links.
    pub fn stats(&self) -> Result<Stats> {
        let virtuals = self.virtuals()?;
        let index = self.index()?;
        let blocks = self.corpus()?.len();

        let mut entities = 0;
        let mut journals = 0;
        let mut links = 0;
        let mut ambiguous = BTreeSet::new();
        // Entities with a link in or out, to a real entity or to nowhere.
        let mut connected = BTreeSet::new();
        let entity_of = |block| index.entity_of(block).map(|e| index.canonical(e));
        for entity in index.entities() {
            entities += 1;
            if matches!(entity.identity(), Some(Identity::Journal(_))) {
                journals += 1;
            }
            for backlink in index.backlinks(entity.id) {
                links += 1;
                if backlink.ambiguous {
                    ambiguous.insert(backlink.source);
                }
                if let Some(source) = entity_of(backlink.source)
                    && source != entity.id
                {
                    connected.extend([source, entity.id]);
                }
            }
        }

        let mut stats = Vec::with_capacity(virtuals.len());
        let mut links_to_nowhere = 0;
        for v in virtuals {
            let from: BTreeSet<_> = v.sources.iter().filter_map(|&b| entity_of(b)).collect();
            connected.extend(&from);
            links_to_nowhere += v.sources.len();
            let key_uses =
                virtual_entity::name_of(&v.id).map_or(0, |n| index.undefined_key(n).len());
            stats.push(VirtualStat {
                id: v.id,
                title: v.title,
                links: v.sources.len(),
                linked_from: from.len(),
                key_uses,
            });
        }
        // Most wanted first, and by name among equals, as everything is listed.
        stats.sort_by_cached_key(|s| {
            (
                std::cmp::Reverse(s.links),
                s.title.to_lowercase(),
                s.id.clone(),
            )
        });

        let orphans = index
            .entities()
            .filter(|e| !matches!(e.identity(), Some(Identity::Journal(_))))
            .filter(|e| !connected.contains(&e.id))
            .count();

        Ok(Stats {
            blocks,
            entities,
            journals,
            merged_duplicates: index.duplicates().map(|g| g.len() - 1).sum(),
            blobs: index.assets().count(),
            links,
            links_to_nowhere,
            ambiguous: ambiguous.len(),
            orphans,
            virtuals: stats,
        })
    }

    /// Checks the graph against what is derived from it, and what is written
    /// against what it says. It reads the whole graph again, and builds a
    /// second index to hold the one in use to.
    pub fn check(&self) -> Result<Check> {
        // Both are up to date after this, and stay so while nothing is edited.
        drop(self.index()?);
        let blocks: BTreeSet<BlockId> = self.corpus()?.iter().map(|b| b.id).collect();
        let virtuals = self.virtuals()?;
        let index = self.index()?;
        let integrity = vec![
            outcome(
                "The blocks held are the graph's blocks",
                diff_blocks(&self.corpus()?, &self.graph.scan()?),
            ),
            outcome(
                "The index agrees with one built afresh",
                diff_indexes(&index, &Index::from_scan(&self.corpus()?)),
            ),
            outcome(
                "Every link is from a block and to an entity",
                dangling(&index, &blocks),
            ),
            outcome(
                "Every virtual entity is well formed",
                virtuals
                    .iter()
                    .flat_map(|v| malformed(v, &index, &blocks))
                    .collect(),
            ),
            outcome("Every ##blob line can be read", self.unreadable_blobs()?),
        ];
        drop(index);
        let content = vec![outcome("Properties fit their schemas", self.flagged()?)];
        Ok(Check { integrity, content })
    }

    /// `##blob` lines that name no file, which the page of their entity
    /// flags.
    fn unreadable_blobs(&self) -> Result<Vec<Problem>> {
        let index = self.index()?;
        let (lines, readable) = (index.blob_lines().count(), index.assets().count());
        Ok(if lines > readable {
            vec![Problem::new(format!(
                "{} of {lines} ##blob lines can't be read",
                lines - readable
            ))]
        } else {
            Vec::new()
        })
    }

    /// What every entity's own properties say is wrong with them, in words.
    fn flagged(&self) -> Result<Vec<Problem>> {
        let ids: Vec<_> = self
            .index()?
            .entities()
            .map(|e| (e.id, e.name.clone()))
            .collect();
        let mut problems = Vec::new();
        for (id, name) in ids {
            for message in self.flag(id, None)? {
                problems.push(Problem::about(id, format!("{name}: {message}")));
            }
        }
        Ok(problems)
    }
}

/// How the blocks held differ from the graph's: the first few that do.
pub(crate) fn diff_blocks(held: &[Scanned], graph: &[Scanned]) -> Vec<Problem> {
    if held == graph {
        return Vec::new();
    }
    let mut problems = Vec::new();
    if held.len() != graph.len() {
        problems.push(Problem::new(format!(
            "{} blocks held, and {} in the graph",
            held.len(),
            graph.len()
        )));
    }
    let held_by_id: BTreeMap<_, _> = held.iter().map(|b| (b.id, b)).collect();
    let graph_by_id: BTreeMap<_, _> = graph.iter().map(|b| (b.id, b)).collect();
    for (id, block) in &graph_by_id {
        match held_by_id.get(id) {
            None => problems.push(Problem::new(format!("{id} is in the graph and not held"))),
            Some(h) if h.text != block.text => {
                problems.push(Problem::new(format!("{id} holds old text: {:?}", h.text)));
            }
            Some(h) if h.parent != block.parent => {
                problems.push(Problem::new(format!("{id} is held under another parent")));
            }
            Some(_) => {}
        }
    }
    problems.extend(
        held_by_id
            .keys()
            .filter(|id| !graph_by_id.contains_key(id))
            .map(|id| Problem::new(format!("{id} is held and no longer in the graph"))),
    );
    if problems.is_empty() {
        // The same blocks with the same text, so only the order is wrong, which
        // is the order of every list of links.
        problems.push(Problem::new("the same blocks, in another order"));
    }
    problems
}

/// How `live` differs from `fresh`, an index built from the same blocks. The
/// live one is kept up to date a block at a time while typing, so this is where
/// that going wrong shows.
pub(crate) fn diff_indexes(live: &Index, fresh: &Index) -> Vec<Problem> {
    let mut problems = Vec::new();
    let named = |index: &Index| -> BTreeMap<BlockId, String> {
        index.entities().map(|e| (e.id, e.name.clone())).collect()
    };
    let (live_entities, fresh_entities) = (named(live), named(fresh));
    for (id, name) in &live_entities {
        match fresh_entities.get(id) {
            None => problems.push(Problem::about(
                id,
                format!("{name} isn't an entity in a fresh index"),
            )),
            Some(other) if other != name => {
                problems.push(Problem::about(
                    id,
                    format!("{name} is called {other} in a fresh index"),
                ));
            }
            Some(_) => {
                let (a, b) = (live.backlinks(*id), fresh.backlinks(*id));
                if a != b {
                    problems.push(Problem::about(
                        id,
                        format!(
                            "{name} has {} backlinks, and {} in a fresh index",
                            a.len(),
                            b.len()
                        ),
                    ));
                }
                if live.entity(*id).map(|e| &e.properties)
                    != fresh.entity(*id).map(|e| &e.properties)
                {
                    problems.push(Problem::about(
                        id,
                        format!("{name} has other properties in a fresh index"),
                    ));
                }
            }
        }
    }
    for (id, name) in &fresh_entities {
        if !live_entities.contains_key(id) {
            problems.push(Problem::about(
                id,
                format!("{name} is an entity in a fresh index only"),
            ));
        }
    }

    let virtual_of = |name: &str| format!("~{}", normalise_name(name));
    let mentions = |index: &Index| -> BTreeMap<String, Vec<_>> {
        index
            .mentions()
            .map(|(n, m)| (n.to_owned(), m.to_vec()))
            .collect()
    };
    let (live_mentions, fresh_mentions) = (mentions(live), mentions(fresh));
    for name in live_mentions
        .keys()
        .chain(fresh_mentions.keys())
        .collect::<BTreeSet<_>>()
    {
        let (a, b) = (live_mentions.get(name), fresh_mentions.get(name));
        if a != b {
            problems.push(Problem::about(
                virtual_of(name),
                format!(
                    "{name} is linked {} times, and {} in a fresh index",
                    a.map_or(0, Vec::len),
                    b.map_or(0, Vec::len)
                ),
            ));
        }
    }
    let keys = |index: &Index| -> BTreeMap<String, Vec<_>> {
        index
            .undefined_keys()
            .map(|(n, u)| (n.to_owned(), u.to_vec()))
            .collect()
    };
    let (live_keys, fresh_keys) = (keys(live), keys(fresh));
    for name in live_keys
        .keys()
        .chain(fresh_keys.keys())
        .collect::<BTreeSet<_>>()
    {
        let (a, b) = (live_keys.get(name), fresh_keys.get(name));
        if a != b {
            problems.push(Problem::about(
                virtual_of(name),
                format!(
                    "{name} is used as a key {} times, and {} in a fresh index",
                    a.map_or(0, Vec::len),
                    b.map_or(0, Vec::len)
                ),
            ));
        }
    }

    if live.assets().map(|(b, _)| b).collect::<Vec<_>>()
        != fresh.assets().map(|(b, _)| b).collect::<Vec<_>>()
    {
        problems.push(Problem::new("the blobs listed differ from a fresh index's"));
    }
    problems
}

/// Links from a block that is not in `blocks`, or that is in no entity.
pub(crate) fn dangling(index: &Index, blocks: &BTreeSet<BlockId>) -> Vec<Problem> {
    let mut problems = Vec::new();
    for entity in index.entities() {
        for backlink in index.backlinks(entity.id) {
            let source = backlink.source;
            if !blocks.contains(&source) || index.entity_of(source).is_none() {
                problems.push(Problem::about(
                    entity.id,
                    format!(
                        "{} has a link from {source}, which is in no entity",
                        entity.name
                    ),
                ));
            }
        }
    }
    problems
}

/// What can't be true of the virtual entity `v`, given the blocks there are.
pub(crate) fn malformed(v: &Virtual, index: &Index, blocks: &BTreeSet<BlockId>) -> Vec<Problem> {
    let name = virtual_entity::name_of(&v.id).unwrap_or_default();
    let mut said = Vec::new();
    if virtual_entity::id_of(&v.title) != v.id {
        said.push(format!("its title, {:?}, makes another ID", v.title));
    }
    if !index.named(name).is_empty() {
        said.push("an entity has this name, so it is not virtual".to_owned());
    }
    if v.sources.is_empty() && index.undefined_key(name).is_empty() {
        said.push("nothing links to it or uses it as a key".to_owned());
    }
    said.extend(
        v.sources
            .iter()
            .filter(|s| !blocks.contains(s) || index.entity_of(**s).is_none())
            .map(|s| format!("linked from {s}, which is in no entity")),
    );
    said.into_iter()
        .map(|m| Problem::about(&v.id, format!("{}: {m}", v.id)))
        .collect()
}
