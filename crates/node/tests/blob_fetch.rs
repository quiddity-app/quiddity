//! Blobs sent between SQLite blob stores, against iroh-blobs' own `MemStore`
//! as the oracle.

mod support;

use bao_tree::{ChunkNum, ChunkRanges};
use iroh_blobs::Hash;
use iroh_blobs::api::Store as BlobStore;
use iroh_blobs::api::blobs::BlobStatus;
use iroh_blobs::store::mem::MemStore;
use proptest::prelude::*;
use quiddity_node::blob_store::{self, SqliteBlobs};
use support::{EDGES, bytes, graph, runtime};

/// Sends the chunks `part` of a blob from one store to another.
async fn send(from: &BlobStore, to: &BlobStore, hash: Hash, part: ChunkRanges) {
    let encoded = from
        .export_bao(hash, part.clone())
        .bao_to_vec()
        .await
        .unwrap();
    to.import_bao_bytes(hash, part, encoded).await.unwrap();
}

fn chunks(from: u64, to: u64) -> ChunkRanges {
    ChunkRanges::from(ChunkNum(from)..ChunkNum(to))
}

/// A source with a 200,001-byte blob in it, and an empty target beside it.
async fn pair() -> (SqliteBlobs, SqliteBlobs, Hash, Vec<tempfile::TempDir>) {
    let (a, from) = graph();
    let (b, to) = graph();
    let source = blob_store::open(&from).unwrap();
    let target = blob_store::open(&to).unwrap();
    let hash = source.add_bytes(bytes(200_001, 7)).await.unwrap().hash;
    (source, target, hash, vec![a, b])
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// A blob sent from one store to another, in any pieces and any order, is
    /// whole at the end, kept in the file, and not before.
    #[test]
    fn fetched_blobs_are_kept_when_whole(
        len in prop_oneof![1usize..3000, 1usize..120_000, prop::sample::select(EDGES[1..].to_vec())],
        seed in any::<u8>(),
        cut in 0u64..130,
    ) {
        runtime().block_on(async {
            let (_a, from) = graph();
            let (_b, to) = graph();
            let source = blob_store::open(&from).unwrap();
            let target = blob_store::open(&to).unwrap();
            let data = bytes(len, seed);
            let hash = source.add_bytes(data.clone()).await.unwrap().hash;

            for part in [chunks(0, cut), chunks(cut, u64::MAX / 2)] {
                if !part.is_empty() {
                    send(&source, &target, hash, part).await;
                }
            }
            // Give the write that follows completion time to land.
            target.wait_idle().await.unwrap();
            assert!(target.has(hash).await.unwrap(), "whole, so has");
            assert_eq!(target.get_bytes(hash).await.unwrap().to_vec(), data);
            target.shutdown().await.unwrap();
            drop(target);

            // It is in the file: a store opened again on it has the blob.
            let again = blob_store::open(&to).unwrap();
            assert_eq!(again.get_bytes(hash).await.unwrap().to_vec(), data);
            // And serves it on, byte for byte as the original does.
            let ours = again.export_bao(hash, ChunkRanges::all()).bao_to_vec().await.unwrap();
            let original = source.export_bao(hash, ChunkRanges::all()).bao_to_vec().await.unwrap();
            assert_eq!(ours, original);
        });
    }
}

/// Part of a blob is not the blob: it is reported as partial, is not listed,
/// and is not in the file, so a crash loses it and it is fetched again.
#[test]
fn part_of_a_blob_is_not_kept() {
    runtime().block_on(async {
        let (source, target, hash, _dirs) = pair().await;
        send(&source, &target, hash, chunks(0, 32)).await;

        assert!(!target.has(hash).await.unwrap());
        assert!(matches!(
            target.status(hash).await.unwrap(),
            BlobStatus::Partial { .. }
        ));
        assert!(target.list().hashes().await.unwrap().is_empty());
    });
}

/// What is held of a blob can be served on, as the whole of it can: the
/// pieces proved are the same as the source gives for those chunks. Only whole
/// groups of 16 chunks can be proved, so what is held is held in groups, and
/// iroh-blobs' own store refuses the same partial groups.
#[test]
fn a_partial_blob_is_served_as_it_is_held() {
    runtime().block_on(async {
        let (source, target, hash, _dirs) = pair().await;
        send(&source, &target, hash, chunks(0, 48)).await;
        send(&source, &target, hash, chunks(96, 128)).await;

        for part in [
            chunks(0, 48),
            chunks(96, 128),
            chunks(5, 30),
            chunks(100, 110),
        ] {
            let ours = target
                .export_bao(hash, part.clone())
                .bao_to_vec()
                .await
                .unwrap();
            let original = source.export_bao(hash, part).bao_to_vec().await.unwrap();
            assert_eq!(ours, original);
        }
        // What isn't held can't be served.
        let missing = target.export_bao(hash, chunks(50, 90)).bao_to_vec().await;
        assert!(missing.is_err());
    });
}

/// What a store says it holds of a blob is what `MemStore` says of the same
/// pieces, whole, partial and absent.
#[test]
fn observing_reports_what_is_held() {
    runtime().block_on(async {
        let (source, target, hash, _dirs) = pair().await;
        let oracle: BlobStore = MemStore::new().into();
        let report = |b: iroh_blobs::api::blobs::Bitfield| format!("{b:?}");

        let absent = report(target.observe(hash).await.unwrap());
        assert_eq!(absent, report(oracle.observe(hash).await.unwrap()));

        for part in [chunks(0, 32), chunks(64, 70)] {
            send(&source, &target, hash, part.clone()).await;
            send(&source, &oracle, hash, part).await;
            let ours = target.observe(hash).await.unwrap();
            assert_eq!(report(ours), report(oracle.observe(hash).await.unwrap()));
        }
        let whole = source.observe(hash).await.unwrap();
        assert!(whole.is_complete());
        assert_eq!(whole.size(), 200_001);
    });
}

/// A blob that was only observed has no data to serve, whatever else is said
/// of it.
#[test]
fn an_observed_blob_is_not_served() {
    runtime().block_on(async {
        let (_dir, graph) = graph();
        let blobs = blob_store::open(&graph).unwrap();
        let hash = Hash::new(b"not here yet");
        blobs.observe(hash).await.unwrap();
        assert!(
            blobs
                .export_bao(hash, ChunkRanges::all())
                .bao_to_vec()
                .await
                .is_err()
        );
        assert!(!blobs.has(hash).await.unwrap());
    });
}

/// Deleting forgets what was fetched, in part or whole, from memory as well as
/// from the file.
#[test]
fn deleting_forgets_what_was_fetched() {
    runtime().block_on(async {
        let (source, target, hash, _dirs) = pair().await;
        send(&source, &target, hash, chunks(0, 32)).await;
        target.delete(vec![hash]).await.unwrap();
        assert!(matches!(
            target.status(hash).await.unwrap(),
            BlobStatus::NotFound
        ));

        send(&source, &target, hash, ChunkRanges::all()).await;
        target.wait_idle().await.unwrap();
        assert!(target.has(hash).await.unwrap());
        target.delete(vec![hash]).await.unwrap();
        assert!(!target.has(hash).await.unwrap());
        assert!(matches!(
            target.status(hash).await.unwrap(),
            BlobStatus::NotFound
        ));
    });
}

/// A blob a peer says is over the limit is refused, and one of exactly the
/// limit is kept: the size is the peer's claim, so it is checked before any
/// data is held for it.
#[test]
fn a_fetch_over_the_limit_is_refused() {
    runtime().block_on(async {
        let (_a, from) = graph();
        let (_b, to) = graph();
        let source = blob_store::open(&from).unwrap();
        let target = blob_store::open_limited(&to, 5000).unwrap();
        let at = source.add_bytes(bytes(5000, 3)).await.unwrap().hash;
        let over = source.add_bytes(bytes(5001, 3)).await.unwrap().hash;

        send(&source, &target, at, ChunkRanges::all()).await;
        target.wait_idle().await.unwrap();
        assert!(target.has(at).await.unwrap());

        let encoded = source
            .export_bao(over, ChunkRanges::all())
            .bao_to_vec()
            .await
            .unwrap();
        let sent = target
            .import_bao_bytes(over, ChunkRanges::all(), encoded)
            .await;
        assert!(sent.is_err());
        assert!(!target.has(over).await.unwrap());
        assert!(matches!(
            target.status(over).await.unwrap(),
            BlobStatus::NotFound
        ));
    });
}
