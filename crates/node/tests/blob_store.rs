//! The SQLite blob store against iroh-blobs' own `MemStore`, which is the
//! oracle: whatever one says about a blob, the other must too.

mod support;

use bao_tree::{ChunkNum, ChunkRanges};
use iroh_blobs::Hash;
use iroh_blobs::api::Store as BlobStore;
use iroh_blobs::api::blobs::BlobStatus;
use iroh_blobs::store::mem::MemStore;
use proptest::prelude::*;
use quiddity_node::blob_store;
use support::{EDGES, bytes, graph, runtime};

/// What goes in comes out, whole and in part, whatever its size, and is still
/// there when the file is opened again.
#[test]
fn blobs_are_kept_in_the_file() {
    runtime().block_on(async {
        let (_dir, graph) = graph();
        let blobs = blob_store::open(&graph).unwrap();
        let mut kept = Vec::new();
        for (i, len) in EDGES.into_iter().enumerate() {
            let data = bytes(len, i as u8);
            let hash = blobs.add_bytes(data.clone()).await.unwrap().hash;
            assert_eq!(hash, Hash::new(&data), "{len} bytes");
            assert_eq!(
                blobs.get_bytes(hash).await.unwrap().to_vec(),
                data,
                "{len} bytes"
            );
            assert!(blobs.has(hash).await.unwrap());
            assert!(matches!(
                blobs.status(hash).await.unwrap(),
                BlobStatus::Complete { size } if size == len as u64
            ));
            kept.push((hash, data));
        }
        blobs.shutdown().await.unwrap();
        drop(blobs);

        let again = blob_store::open(&graph).unwrap();
        for (hash, data) in kept {
            assert_eq!(again.get_bytes(hash).await.unwrap().to_vec(), data);
        }
    });
}

/// A hash that was never given is reported as absent, and is not listed.
#[test]
fn absent_blobs_are_absent() {
    runtime().block_on(async {
        let (_dir, graph) = graph();
        let blobs = blob_store::open(&graph).unwrap();
        let hash = Hash::new(b"absent");
        assert!(!blobs.has(hash).await.unwrap());
        assert!(matches!(
            blobs.status(hash).await.unwrap(),
            BlobStatus::NotFound
        ));
        assert!(blobs.get_bytes(hash).await.is_err());
        assert!(blobs.list().hashes().await.unwrap().is_empty());
    });
}

/// Deleting removes a blob from the file, and only that one. Putting the
/// same bytes twice keeps one.
#[test]
fn deleting_removes_only_what_is_named() {
    runtime().block_on(async {
        let (_dir, graph) = graph();
        let blobs = blob_store::open(&graph).unwrap();
        let a = blobs.add_bytes(&b"one"[..]).await.unwrap().hash;
        let b = blobs.add_bytes(&b"two"[..]).await.unwrap().hash;
        blobs.add_bytes(&b"one"[..]).await.unwrap();
        assert_eq!(blobs.list().hashes().await.unwrap().len(), 2);
        blobs
            .delete(vec![a, Hash::new(b"never stored")])
            .await
            .unwrap();
        assert!(!blobs.has(a).await.unwrap());
        assert!(blobs.has(b).await.unwrap());
    });
}

fn ranges(parts: &[(u64, u64)]) -> ChunkRanges {
    parts
        .iter()
        .map(|(a, len)| ChunkRanges::from(ChunkNum(*a)..ChunkNum(a + len + 1)))
        .fold(ChunkRanges::empty(), |all, one| all | one)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// What is proved of any part of any blob is, byte for byte, what
    /// `MemStore` proves: the encoding is iroh's, so a peer can't tell.
    #[test]
    fn the_encoding_is_iroh_s(
        len in prop_oneof![Just(0usize), 1usize..3000, 0usize..120_000, prop::sample::select(EDGES.to_vec())],
        seed in any::<u8>(),
        parts in prop::collection::vec((0u64..130, 0u64..40), 0..4),
    ) {
        runtime().block_on(async {
            let (_dir, graph) = graph();
            let sqlite = blob_store::open(&graph).unwrap();
            let oracle: BlobStore = MemStore::new().into();
            let data = bytes(len, seed);
            let hash = sqlite.add_bytes(data.clone()).await.unwrap().hash;
            assert_eq!(oracle.add_bytes(data).await.unwrap().hash, hash);

            let wanted = if parts.is_empty() { ChunkRanges::all() } else { ranges(&parts) };
            let ours = sqlite.export_bao(hash, wanted.clone()).bao_to_vec().await;
            let theirs = oracle.export_bao(hash, wanted).bao_to_vec().await;
            assert_eq!(ours.map_err(|e| e.to_string()), theirs.map_err(|e| e.to_string()));
        });
    }
}

/// The empty blob is always there, and is never a row.
#[test]
fn the_empty_blob_is_always_there() {
    runtime().block_on(async {
        let (_dir, graph) = graph();
        let blobs = blob_store::open(&graph).unwrap();
        assert!(blobs.has(Hash::EMPTY).await.unwrap());
        assert!(blobs.get_bytes(Hash::EMPTY).await.unwrap().is_empty());
        assert!(matches!(
            blobs.status(Hash::EMPTY).await.unwrap(),
            BlobStatus::Complete { size: 0 }
        ));
        assert_eq!(blobs.add_bytes(Vec::new()).await.unwrap().hash, Hash::EMPTY);
        assert!(blobs.list().hashes().await.unwrap().is_empty());
    });
}

/// Any part of a blob is read as asked: whole, from an offset and for a
/// length, across the steps it is read in, and to the end.
#[test]
fn ranges_are_read_as_asked() {
    use std::io::SeekFrom;
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    runtime().block_on(async {
        let (_dir, graph) = graph();
        let blobs = blob_store::open(&graph).unwrap();
        for (i, len) in EDGES.into_iter().enumerate().skip(1) {
            let data = bytes(len, i as u8);
            let hash = blobs.add_bytes(data.clone()).await.unwrap().hash;

            let mut whole = Vec::new();
            blobs.reader(hash).read_to_end(&mut whole).await.unwrap();
            assert_eq!(whole, data, "{len} bytes");

            for (offset, want) in [(0, 1), (1, 3000), (1023, 2), (1024, 1025), (5000, 70_000)] {
                if offset >= len {
                    continue;
                }
                let want = want.min(len - offset);
                let mut reader = blobs.reader(hash);
                reader.seek(SeekFrom::Start(offset as u64)).await.unwrap();
                let mut part = vec![0; want];
                reader.read_exact(&mut part).await.unwrap();
                assert_eq!(
                    part,
                    data[offset..offset + want],
                    "{want} from {offset} of {len}"
                );
            }
            // A response is read to its end, and ends: it is not padded with
            // empty pieces for ever, which a reader that stops early never sees.
            for (from, to) in [
                (0, len),
                (1, len.min(3000)),
                (1000, len.min(2100)),
                (len - 1, len),
            ] {
                if from >= to {
                    continue;
                }
                let part = blobs
                    .export_ranges(hash, from as u64..to as u64)
                    .concatenate()
                    .await;
                assert_eq!(part.unwrap(), data[from..to], "{from}..{to} of {len}");
            }
            let mut tail = Vec::new();
            let mut reader = blobs.reader(hash);
            reader.seek(SeekFrom::Start(len as u64 - 1)).await.unwrap();
            reader.read_to_end(&mut tail).await.unwrap();
            assert_eq!(tail, data[len - 1..], "the last byte of {len}");
        }
    });
}

/// A blob over the limit is refused and not kept; one of exactly the limit is
/// kept. (The limit is small here: the real one is a billion bytes.)
#[test]
fn blobs_over_the_limit_are_refused() {
    runtime().block_on(async {
        let (_dir, graph) = graph();
        let blobs = blob_store::open_limited(&graph, 100).unwrap();
        let at = bytes(100, 1);
        let over = bytes(101, 1);
        assert_eq!(
            blobs.add_bytes(at.clone()).await.unwrap().hash,
            Hash::new(&at)
        );
        assert!(blobs.add_bytes(over.clone()).await.is_err());
        assert!(!blobs.has(Hash::new(&over)).await.unwrap());
        assert_eq!(blobs.list().hashes().await.unwrap().len(), 1);
    });
}

/// What this store doesn't do says so, rather than hanging or pretending.
#[test]
fn files_are_not_imported_by_path() {
    runtime().block_on(async {
        let (dir, graph) = graph();
        let blobs = blob_store::open(&graph).unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, b"contents").unwrap();
        let err = blobs.add_path(&file).await.unwrap_err();
        // iroh's error keeps what caused it, not in how it is shown.
        let shown = format!("{err:?}");
        assert!(shown.contains("not supported"), "{shown}");
        assert!(blobs.list().hashes().await.unwrap().is_empty());
    });
}

/// Asking for what is not there is an error for the encoding too, and the
/// empty blob needs nothing to be encoded.
#[test]
fn encoding_what_is_absent_is_an_error() {
    runtime().block_on(async {
        let (_dir, graph) = graph();
        let blobs = blob_store::open(&graph).unwrap();
        let oracle: BlobStore = MemStore::new().into();
        for hash in [Hash::new(b"absent"), Hash::EMPTY] {
            let ours = blobs
                .export_bao(hash, ChunkRanges::all())
                .bao_to_vec()
                .await;
            let theirs = oracle
                .export_bao(hash, ChunkRanges::all())
                .bao_to_vec()
                .await;
            assert_eq!(ours.is_ok(), theirs.is_ok(), "{hash}");
        }
    });
}

/// Observing a whole blob says it is whole, and how big; one that isn't here
/// says that none of it is, as `MemStore` does.
#[test]
fn observing_a_whole_blob_says_so() {
    runtime().block_on(async {
        let (_dir, graph) = graph();
        let blobs = blob_store::open(&graph).unwrap();
        let hash = blobs.add_bytes(bytes(5000, 2)).await.unwrap().hash;
        let seen = blobs.observe(hash).await.unwrap();
        assert!(seen.is_complete());
        assert_eq!(seen.size(), 5000);
        let absent = blobs.observe(Hash::new(b"absent")).await.unwrap();
        assert!(!absent.is_complete());
        assert_eq!(absent.size(), 0);
    });
}
