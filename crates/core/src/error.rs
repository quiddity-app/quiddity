use crate::{BlockId, NodeId};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Loro(#[from] loro::LoroError),
    #[error(transparent)]
    Encode(#[from] loro::LoroEncodeError),
    #[error("an entity needs a name")]
    EmptyName,
    #[error("block {0} does not exist")]
    NoSuchBlock(BlockId),
    #[error("edit past the end of block {0}'s text")]
    OutOfRange(BlockId),
    #[error("block {0} has no text container")]
    MissingText(BlockId),
    #[error("text update timed out")]
    TextUpdate(#[from] loro::UpdateTimeoutError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("that predates this replica's history, which has been compacted")]
    BeforeHistory,
    #[error("those updates can't be applied here: this replica lacks what they were made on")]
    NotApplied,
    #[error("this graph already has an ID")]
    GraphIdExists,
    #[error("a blank replica has no graph to give an ID to")]
    BlankReplica,
    #[error("that node ID can't be used: Loro keeps it for itself")]
    InvalidNodeId,
    #[error(
        "those changes are by another replica editing as {0}, as this one has: merging them would leave the two different, so they were refused"
    )]
    Twin(NodeId),
    #[error("in quiddity.toml: {0}")]
    Settings(String),
}

pub type Result<T> = std::result::Result<T, Error>;
