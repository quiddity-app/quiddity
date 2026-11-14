//! Blobs kept the way they used to be, in `blobs/` beside the graph, are
//! moved into the graph's file.

mod support;

use std::path::Path;

use iroh_blobs::Hash;
use iroh_blobs::store::fs::FsStore;
use quiddity_node::blob_store;
use quiddity_node::legacy_blobs::adopt;
use support::{bytes, graph, runtime};

/// What an old graph held: a few bytes, a few chunks, and enough that the old
/// store kept it as a file of its own and not inside its database.
const SIZES: [usize; 5] = [1, 1025, 16 * 1024 + 1, 200_001, 5 * 1024 * 1024 + 3];

/// Keeps blobs the old way, in `graph_dir/blobs`.
async fn old_store(graph_dir: &Path, blobs: &[Vec<u8>]) -> Vec<Hash> {
    let store = FsStore::load(graph_dir.join("blobs")).await.unwrap();
    let mut hashes = Vec::new();
    for data in blobs {
        let tag = store.blobs().add_bytes(data.clone()).await.unwrap();
        hashes.push(tag.hash);
    }
    store.shutdown().await.unwrap();
    hashes
}

/// Everything that was kept is kept, whole, in the file; the old directory is
/// set aside with what it held; and doing it again finds nothing to do.
#[test]
fn old_blobs_are_moved_into_the_file() {
    runtime().block_on(async {
        let (dir, graph) = graph();
        let held: Vec<_> = SIZES.iter().map(|n| bytes(*n, *n as u8)).collect();
        let hashes = old_store(dir.path(), &held).await;
        let blobs = blob_store::open(&graph).unwrap();

        assert_eq!(adopt(dir.path(), &blobs).await.unwrap(), SIZES.len());
        for (hash, data) in hashes.iter().zip(&held) {
            assert_eq!(blobs.get_bytes(*hash).await.unwrap().to_vec(), *data);
        }
        assert_eq!(blobs.list().hashes().await.unwrap().len(), SIZES.len());
        assert!(!dir.path().join("blobs").exists());
        assert!(dir.path().join("blobs.migrated/blobs.db").exists());

        assert_eq!(adopt(dir.path(), &blobs).await.unwrap(), 0);
    });
}

/// A graph that never kept a blob has nothing to move and is left as it is:
/// the old store is not made to look for it, and another `blobs/` is not ours.
#[test]
fn there_is_nothing_to_move_without_an_old_store() {
    runtime().block_on(async {
        let (dir, graph) = graph();
        let blobs = blob_store::open(&graph).unwrap();
        assert_eq!(adopt(dir.path(), &blobs).await.unwrap(), 0);
        assert!(!dir.path().join("blobs").exists());

        std::fs::create_dir(dir.path().join("blobs")).unwrap();
        std::fs::write(dir.path().join("blobs/notes"), b"somebody else's").unwrap();
        assert_eq!(adopt(dir.path(), &blobs).await.unwrap(), 0);
        assert_eq!(
            std::fs::read(dir.path().join("blobs/notes")).unwrap(),
            b"somebody else's"
        );
        assert!(!dir.path().join("blobs.migrated").exists());
    });
}

/// When it can't finish, the old directory stays as it was, and what was
/// moved stays moved, so a second try moves the rest and finishes.
#[test]
fn a_move_that_cannot_finish_leaves_the_old_directory() {
    runtime().block_on(async {
        let (dir, graph) = graph();
        let held = [bytes(50, 1), bytes(500, 2)];
        let hashes = old_store(dir.path(), &held).await;

        // The file refuses anything over 100 bytes.
        let small = blob_store::open_limited(&graph, 100).unwrap();
        assert!(adopt(dir.path(), &small).await.is_err());
        assert!(dir.path().join("blobs/blobs.db").exists());
        assert!(!dir.path().join("blobs.migrated").exists());

        // Room for it, but somewhere to put the old directory is taken.
        let blobs = blob_store::open(&graph).unwrap();
        std::fs::create_dir(dir.path().join("blobs.migrated")).unwrap();
        let err = adopt(dir.path(), &blobs).await.unwrap_err();
        assert!(err.to_string().contains("already there"), "{err}");
        assert!(dir.path().join("blobs/blobs.db").exists());
        for (hash, data) in hashes.iter().zip(&held) {
            assert_eq!(blobs.get_bytes(*hash).await.unwrap().to_vec(), *data);
        }

        std::fs::remove_dir(dir.path().join("blobs.migrated")).unwrap();
        assert_eq!(adopt(dir.path(), &blobs).await.unwrap(), 2);
        assert!(!dir.path().join("blobs").exists());
    });
}
