//! Bringing a graph's blobs from where they used to be kept into its file.
//!
//! Before blobs were kept in the graph's SQLite file they were in `blobs/`
//! beside it, in an iroh-blobs filesystem store. [`adopt`] reads that store,
//! keeps every blob it holds in the file, and sets the directory aside as
//! `blobs.migrated/` rather than deleting it, as converting the graph's own
//! files does.

use std::path::Path;

use iroh_blobs::store::fs::FsStore;
use tracing::info;

use crate::blob_store::SqliteBlobs;
use crate::blobs::BlobError;

/// Where the old store was.
const OLD: &str = "blobs";
/// What it is called once its blobs are in the file.
const SET_ASIDE: &str = "blobs.migrated";

/// Keeps the blobs of an old `blobs/` directory in `into`, then sets the
/// directory aside, and says how many it moved. A graph with no such
/// directory has nothing to move, and is left alone.
///
/// Each blob is checked against its hash as it is read, and kept under the
/// hash of what was read, so a blob is only ever kept under its own hash. If
/// any can't be moved the directory is left as it was and the error says so,
/// and the next attempt starts again, passing over what is already kept.
pub async fn adopt(graph_dir: &Path, into: &SqliteBlobs) -> Result<usize, BlobError> {
    let old = graph_dir.join(OLD);
    // The directory is the old store's only if its database is there: another
    // `blobs/` is somebody's, and is not ours to move.
    if !old.join("blobs.db").exists() {
        return Ok(0);
    }
    let store = FsStore::load(&old).await.map_err(BlobError::store)?;
    let moved = move_all(&store, into).await;
    // Stopped either way: its database is locked until its tasks end.
    store.shutdown().await.map_err(BlobError::store)?;
    let moved = moved?;

    let aside = graph_dir.join(SET_ASIDE);
    if aside.exists() {
        return Err(BlobError::Store(format!(
            "{} is already there: move it and try again",
            aside.display()
        )));
    }
    std::fs::rename(&old, &aside)?;
    info!(moved, "blobs moved into the graph's file");
    Ok(moved)
}

async fn move_all(old: &FsStore, into: &SqliteBlobs) -> Result<usize, BlobError> {
    let hashes = old
        .blobs()
        .list()
        .hashes()
        .await
        .map_err(BlobError::store)?;
    let mut moved = 0;
    for hash in hashes {
        let bytes = old
            .blobs()
            .get_bytes(hash)
            .await
            .map_err(BlobError::store)?;
        into.add_bytes(bytes).await.map_err(BlobError::store)?;
        moved += 1;
    }
    Ok(moved)
}
