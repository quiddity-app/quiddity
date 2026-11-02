//! Quiddity's graph model.
//!
//! A graph is a single Loro document holding a tree of blocks. Each block has
//! Markdown text, and that text is the source of truth for the block's
//! properties (see [`quiddity_syntax`]). Root blocks are entities, the
//! equivalent of Logseq or Roam pages: an entity's text is its name, and its
//! first child, if it consists only of `key:: value` lines, holds the
//! entity's properties.
//!
//! Everything derived from the text (names, properties, links, backlinks) is
//! computed by an [`Index`], which can always be rebuilt from the graph.

mod check;
mod devices;
mod effective;
mod entity;
mod error;
pub mod files;
mod graph;
mod index;
mod journal;
mod merge;
pub mod outline;
mod query;
pub mod schema;

pub use check::Flag;
pub use devices::Device;
pub use effective::Item;
pub use error::{Error, Result};
pub use graph::{BlockId, Graph, SCHEMA_VERSION, Version};
pub use index::{Backlink, EntityInfo, Identity, Index, Resolution, canonical_value};
pub use journal::ensure_journal;
pub use merge::merge_duplicates;
pub use quiddity_syntax as syntax;
