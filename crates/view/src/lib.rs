//! What a Quiddity UI shows of a graph, and the edits it makes to it.
//!
//! A [`View`] owns a [`Graph`] and an [`Index`] over it, rebuilt lazily after
//! edits. The queries return plain data (the types here are the wire format,
//! mirrored by `apps/ui/lib/model.ts`), and the edits are the outliner's:
//! splitting, merging, indenting and moving blocks. The UI runs this compiled
//! to WASM (`quiddity-wasm`), so every replica answers from its own copy.

mod edit;
mod fts;
mod links;
mod meta;
mod query;
mod search;

use std::cell::{Cell, Ref, RefCell};
use std::collections::{HashMap, HashSet};

use jiff::Timestamp;
use quiddity_core::{BlockId, Graph, Index, Scanned, Version};

pub use edit::Merged;
pub use fts::BlockHit;
pub use meta::{INVERSE_LIMIT, Inherited, Inverse, InverseNode};
pub use query::{
    Block, Connections, DeviceInfo, Edge, Entity, EntityRef, LinkTarget, Property, PropertyValue,
    Reference, ResolvedLink, Start, Summary,
};
pub use search::{HitProperty, Match, TitleHit, fuzzy_match};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] quiddity_core::Error),
    #[error("{0:?} is not a block")]
    NotABlock(String),
    #[error("{0:?} is not an entity")]
    NotAnEntity(String),
    #[error("{0:?} is identified by its name or date, so it can't be renamed")]
    NotRenamable(String),
    #[error("{0:?} is not a date")]
    NotADate(String),
    #[error("{0:?} is not a link")]
    NotALink(String),
    #[error("{0:?} matches several entities")]
    AmbiguousLink(String),
    #[error("no block has the id {0:?}")]
    NoSuchBlockRef(String),
    #[error("offset {0} is not between characters")]
    BadOffset(usize),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct View {
    graph: Graph,
    index: RefCell<Index>,
    stale: Cell<bool>,
    /// Every block with its text, kept between revisions because reading it
    /// from Loro is the most expensive part of any whole-graph answer.
    corpus: RefCell<Corpus>,
    work: Cell<Work>,
    /// Whether a burst of typing is open, as one undo step.
    typing: Cell<bool>,
    activity: RefCell<Activity>,
    facts: RefCell<Facts>,
}

/// How much of the expensive work a view has done, for finding out whether an
/// edit cost what it should. Each is a count since the view was made.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Work {
    /// Times the whole graph's structure was read.
    pub scans: u32,
    /// Times the index was built from the whole graph.
    pub index_builds: u32,
    /// Blocks whose text `summaries` has had to read the words and tags of.
    pub blocks_summed: u32,
    /// Times `summaries` has had to ask the graph when every block was put
    /// where it is.
    pub placements_read: u32,
}

/// The blocks as [`Graph::scan`] reads them, and what of them is still true.
#[derive(Debug, Default)]
struct Corpus {
    blocks: Vec<Scanned>,
    /// Where each of `blocks` is.
    at: HashMap<BlockId, usize>,
    /// Whether blocks have been created, moved or deleted since `blocks` was
    /// read.
    restructured: bool,
    /// Blocks whose text has changed since, and nothing else has.
    edited: HashSet<BlockId>,
    /// Whether anything at all may have: re-read every text.
    all: bool,
    /// What has happened to `blocks` since the index last looked.
    log: Log,
}

/// How `blocks` has changed since the index was last brought up to date with
/// it.
#[derive(Debug)]
enum Log {
    /// Only these blocks' texts, each from what it was then.
    Edited(HashMap<BlockId, String>),
    /// Read again, so the index is built again.
    Rescanned,
}

impl Default for Log {
    /// Before the index has been built it has seen nothing.
    fn default() -> Self {
        Self::Rescanned
    }
}

/// What an edit changed, so that only that is read again.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Change {
    /// The text of one block, which already existed.
    Text(BlockId),
    /// Blocks made, moved or deleted, with no text of an existing block
    /// changed.
    Structure,
    /// Anything: an import, an undo.
    Anything,
}

/// When blocks' text was last edited, as far as the graph's history says.
/// History only grows, so it is read once and then only what is new.
#[derive(Debug, Default)]
struct Activity {
    seen: Version,
    edited: HashMap<BlockId, Timestamp>,
}

/// What `summaries` works out of blocks, kept while it is still true: reading
/// it again for every block is most of what it costs.
#[derive(Debug, Default)]
struct Facts {
    /// By block: what its text says. Dropped when the text may have changed.
    text: HashMap<BlockId, TextFacts>,
    /// When each block was last made or moved, for those that were recorded.
    /// Typing doesn't change it, so it lasts until blocks may have moved.
    placed: Option<HashMap<BlockId, Timestamp>>,
}

/// What `summaries` takes from a block's text.
#[derive(Debug)]
struct TextFacts {
    /// Words, or none for a block of nothing but properties.
    words: usize,
    tags: Vec<String>,
}

impl View {
    pub fn new(graph: Graph) -> Result<Self> {
        let view = Self {
            graph,
            index: RefCell::default(),
            stale: Cell::new(true),
            corpus: RefCell::new(Corpus {
                all: true,
                ..Corpus::default()
            }),
            work: Cell::default(),
            typing: Cell::new(false),
            activity: RefCell::default(),
            facts: RefCell::default(),
        };
        // Built now, not on first use, so that a graph that can't be read is
        // refused here.
        drop(view.index()?);
        Ok(view)
    }

    /// Whether the graph has no history at all: a replica that has not yet
    /// joined a graph. Nothing should be made in one, since an edit there
    /// rests on history the graph it joins won't have.
    pub fn is_blank(&self) -> bool {
        self.graph.version() == quiddity_core::Version::default()
    }

    /// The graph, for syncing. Edits made to it directly must be followed by
    /// [`View::changed`].
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// Reads the history there is to read since last time.
    fn refresh_activity(&self) -> Result<()> {
        let mut activity = self.activity.borrow_mut();
        let now = self.graph.version();
        for (block, at) in self.graph.text_edits_since(&activity.seen)? {
            let latest = activity.edited.entry(block).or_insert(at);
            *latest = (*latest).max(at);
        }
        activity.seen = now;
        Ok(())
    }

    /// Records that the graph changed, from an edit or an import, so the
    /// index is rebuilt before it is next used.
    pub fn changed(&self) {
        self.changed_by(Change::Anything);
    }

    pub(crate) fn changed_by(&self, change: Change) {
        self.stale.set(true);
        let mut corpus = self.corpus.borrow_mut();
        match change {
            Change::Text(block) => {
                corpus.edited.insert(block);
            }
            Change::Structure => corpus.restructured = true,
            Change::Anything => corpus.all = true,
        }
    }

    fn count(&self, count: impl FnOnce(&mut Work)) {
        let mut work = self.work.get();
        count(&mut work);
        self.work.set(work);
    }

    /// What this view has had to do so far. See [`Work`].
    pub fn work(&self) -> Work {
        self.work.get()
    }

    /// Brings the corpus up to date with the graph, reading as little as the
    /// edits since say can have changed.
    fn sync_corpus(&self) -> Result<()> {
        {
            let corpus = self.corpus.borrow();
            if !(corpus.all || corpus.restructured || !corpus.edited.is_empty()) {
                return Ok(());
            }
        }
        let mut corpus = self.corpus.borrow_mut();
        let synced = if corpus.all || corpus.restructured {
            self.rescan(&mut corpus)
        } else {
            self.reread(&mut corpus)
        };
        if synced.is_err() {
            // Whatever was half done, read everything next time.
            corpus.all = true;
        }
        synced
    }

    /// Reads the structure again, and the text of blocks that may have
    /// changed or are new.
    fn rescan(&self, corpus: &mut Corpus) -> Result<()> {
        let mut known = HashMap::new();
        if !corpus.all {
            let edited = std::mem::take(&mut corpus.edited);
            known.extend(
                std::mem::take(&mut corpus.blocks)
                    .into_iter()
                    .filter(|b| !edited.contains(&b.id))
                    .map(|b| (b.id, b.text)),
            );
        }
        {
            // Keep what was worked out of a text that is still the text.
            let mut facts = self.facts.borrow_mut();
            facts.text.retain(|block, _| known.contains_key(block));
            facts.placed = None;
        }
        corpus.blocks = self.graph.scan_reusing(&mut known)?;
        self.count(|w| w.scans += 1);
        corpus.at = corpus
            .blocks
            .iter()
            .enumerate()
            .map(|(at, b)| (b.id, at))
            .collect();
        corpus.edited.clear();
        corpus.restructured = false;
        corpus.all = false;
        corpus.log = Log::Rescanned;
        Ok(())
    }

    /// Reads the text of the blocks edited, and nothing else: no block has
    /// been made, moved or deleted, so the rest is where it was.
    fn reread(&self, corpus: &mut Corpus) -> Result<()> {
        for block in std::mem::take(&mut corpus.edited) {
            let Some(&at) = corpus.at.get(&block) else {
                // Not a block the corpus knows: read everything.
                corpus.all = true;
                return self.rescan(corpus);
            };
            self.facts.borrow_mut().text.remove(&block);
            let new = self.graph.text(block)?;
            let old = std::mem::replace(&mut corpus.blocks[at].text, new);
            if let Log::Edited(edits) = &mut corpus.log {
                // The index knows the first text, not any in between.
                edits.entry(block).or_insert(old);
            }
        }
        Ok(())
    }

    /// Every block with its text, in document order, as of now.
    fn corpus(&self) -> Result<Ref<'_, [Scanned]>> {
        self.sync_corpus()?;
        Ok(Ref::map(self.corpus.borrow(), |c| c.blocks.as_slice()))
    }

    /// The index, made up to date first. An edit to an ordinary block's text
    /// is applied to it where it is, which costs nothing like building it
    /// again; anything else builds it again from the corpus.
    fn index(&self) -> Result<Ref<'_, Index>> {
        if self.stale.get() {
            self.sync_corpus()?;
            let log = std::mem::replace(
                &mut self.corpus.borrow_mut().log,
                Log::Edited(HashMap::new()),
            );
            let corpus = self.corpus.borrow();
            let mut index = self.index.borrow_mut();
            let updated = match &log {
                Log::Edited(edits) => edits.iter().all(|(block, old)| {
                    corpus
                        .at
                        .get(block)
                        .is_some_and(|&at| index.update_text(*block, old, &corpus.blocks[at].text))
                }),
                Log::Rescanned => false,
            };
            if !updated {
                self.count(|w| w.index_builds += 1);
                *index = Index::from_scan(&corpus.blocks);
            }
            self.stale.set(false);
        }
        Ok(self.index.borrow())
    }

    fn block_id(&self, id: &str) -> Result<BlockId> {
        let bad = || Error::NotABlock(id.to_owned());
        let block: BlockId = id.parse().map_err(|_| bad())?;
        if self.graph.contains(block) {
            Ok(block)
        } else {
            Err(bad())
        }
    }
}

#[cfg(test)]
mod tests;
