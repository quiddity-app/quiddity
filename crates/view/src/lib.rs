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
mod query;
mod search;

use std::cell::{Cell, Ref, RefCell};

use quiddity_core::{BlockId, Graph, Index};

pub use edit::Merged;
pub use fts::BlockHit;
pub use query::{
    Block, Connections, Edge, Entity, EntityRef, LinkTarget, Property, PropertyValue, Reference,
    Summary,
};
pub use search::{Match, TitleHit, fuzzy_match};

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
    #[error("offset {0} is not between characters")]
    BadOffset(usize),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct View {
    graph: Graph,
    index: RefCell<Index>,
    stale: Cell<bool>,
    /// Whether a burst of typing is open, as one undo step.
    typing: Cell<bool>,
}

impl View {
    pub fn new(graph: Graph) -> Result<Self> {
        let index = Index::build(&graph)?;
        Ok(Self {
            graph,
            index: RefCell::new(index),
            stale: Cell::new(false),
            typing: Cell::new(false),
        })
    }

    /// The graph, for syncing. Edits made to it directly must be followed by
    /// [`View::changed`].
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// Records that the graph changed, from an edit or an import, so the
    /// index is rebuilt before it is next used.
    pub fn changed(&self) {
        self.stale.set(true);
    }

    fn index(&self) -> Result<Ref<'_, Index>> {
        if self.stale.replace(false) {
            *self.index.borrow_mut() = Index::build(&self.graph)?;
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
