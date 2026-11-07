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
    /// Whether a burst of typing is open, as one undo step.
    typing: Cell<bool>,
    activity: RefCell<Activity>,
}

/// The blocks as [`Graph::scan`] reads them, and what of them is still true.
#[derive(Debug, Default)]
struct Corpus {
    blocks: Vec<Scanned>,
    /// Whether blocks have been created, moved or deleted since `blocks` was
    /// read.
    restructured: bool,
    /// Blocks whose text has changed since.
    edited: HashSet<BlockId>,
    /// Whether anything at all may have: re-read every text.
    all: bool,
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
            typing: Cell::new(false),
            activity: RefCell::default(),
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
            Change::Structure => {}
            Change::Anything => corpus.all = true,
        }
        corpus.restructured = true;
    }

    /// Every block with its text, in document order, as of now. Only what
    /// has changed since the last call is read from the graph again.
    fn corpus(&self) -> Result<Ref<'_, [Scanned]>> {
        let behind = {
            let corpus = self.corpus.borrow();
            corpus.restructured || corpus.all
        };
        if behind {
            let mut corpus = self.corpus.borrow_mut();
            {
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
                corpus.blocks = self.graph.scan_reusing(&mut known)?;
                corpus.edited.clear();
                corpus.restructured = false;
                corpus.all = false;
            }
        }
        Ok(Ref::map(self.corpus.borrow(), |c| c.blocks.as_slice()))
    }

    fn index(&self) -> Result<Ref<'_, Index>> {
        if self.stale.get() {
            let built = Index::from_scan(&self.corpus()?);
            *self.index.borrow_mut() = built;
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
