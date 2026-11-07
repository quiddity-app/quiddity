//! Deleting blobs from a real store: exactly those named, and no others.

use std::collections::BTreeSet;

use quiddity_node::blobs::{Blobs, Hash, sweep};

fn bytes(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(seed).wrapping_add(seed))
        .collect()
}

#[tokio::test]
async fn only_the_doomed_blobs_go_and_the_rest_are_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let blobs = Blobs::open(dir.path()).await.unwrap();
    // Small ones are kept inline by the store and large ones in files of
    // their own; both kinds, in both fates.
    let sizes = [1usize, 100, 5_000, 70_000, 3_000_000];
    let mut all = Vec::new();
    for (i, size) in sizes.iter().cycle().take(20).enumerate() {
        let data = bytes(i as u8 + 1, *size + i);
        all.push((blobs.put(data.clone()).await.unwrap(), data));
    }
    let doomed: BTreeSet<Hash> = all.iter().step_by(3).map(|(h, _)| *h).collect();
    assert_eq!(doomed.len(), 7);
    blobs.shutdown().await.unwrap();

    sweep(dir.path(), &doomed).await.unwrap();

    let blobs = Blobs::open(dir.path()).await.unwrap();
    for (hash, data) in &all {
        if doomed.contains(hash) {
            assert!(!blobs.has(*hash).await.unwrap(), "{hash} should be gone");
        } else {
            assert_eq!(
                &blobs.get(*hash).await.unwrap().to_vec(),
                data,
                "{hash} kept intact"
            );
        }
    }
    assert_eq!(
        blobs.stored().await.unwrap().len(),
        all.len() - doomed.len()
    );

    // The store is as good as before: new blobs, and the same bytes again.
    let again = blobs.put(all[0].1.clone()).await.unwrap();
    assert_eq!(again, all[0].0);
    assert_eq!(blobs.get(again).await.unwrap().to_vec(), all[0].1);
    blobs.shutdown().await.unwrap();
}

/// Nothing to delete leaves the store alone, and naming what isn't there is
/// not a reason to touch anything else.
#[tokio::test]
async fn nothing_doomed_or_nothing_there_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let blobs = Blobs::open(dir.path()).await.unwrap();
    let a = blobs.put(&b"a"[..]).await.unwrap();
    let b = blobs.put(&b"b"[..]).await.unwrap();
    blobs.shutdown().await.unwrap();

    sweep(dir.path(), &BTreeSet::new()).await.unwrap();
    sweep(dir.path(), &BTreeSet::from([Hash::new(b"never stored")]))
        .await
        .unwrap();

    let blobs = Blobs::open(dir.path()).await.unwrap();
    assert_eq!(blobs.stored().await.unwrap(), BTreeSet::from([a, b]));
    blobs.shutdown().await.unwrap();
}

/// Deleting everything that was stored is allowed, and leaves a working store.
#[tokio::test]
async fn everything_can_be_deleted_when_everything_is_named() {
    let dir = tempfile::tempdir().unwrap();
    let blobs = Blobs::open(dir.path()).await.unwrap();
    let a = blobs.put(&b"a"[..]).await.unwrap();
    blobs.shutdown().await.unwrap();

    sweep(dir.path(), &BTreeSet::from([a])).await.unwrap();

    let blobs = Blobs::open(dir.path()).await.unwrap();
    assert!(blobs.stored().await.unwrap().is_empty());
    blobs.put(&b"a"[..]).await.unwrap();
    assert!(blobs.has(a).await.unwrap());
    blobs.shutdown().await.unwrap();
}

/// The empty file is built into the store: it is always there and never
/// listed, so there is nothing to collect and naming it does no harm.
#[tokio::test]
async fn the_empty_file_is_always_there_and_never_collected() {
    let dir = tempfile::tempdir().unwrap();
    let blobs = Blobs::open(dir.path()).await.unwrap();
    let empty = blobs.put(&b""[..]).await.unwrap();
    let other = blobs.put(&b"other"[..]).await.unwrap();
    assert_eq!(empty, Hash::new(b""));
    assert_eq!(blobs.stored().await.unwrap(), BTreeSet::from([other]));
    blobs.shutdown().await.unwrap();

    sweep(dir.path(), &BTreeSet::from([empty])).await.unwrap();

    let blobs = Blobs::open(dir.path()).await.unwrap();
    assert!(blobs.has(empty).await.unwrap());
    assert!(blobs.get(empty).await.unwrap().is_empty());
    assert!(blobs.has(other).await.unwrap());
    blobs.shutdown().await.unwrap();
}
