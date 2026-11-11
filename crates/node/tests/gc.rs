//! Deciding which files may go: the ledger, the rule, and the collector over
//! real stores.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use jiff::{SignedDuration, Timestamp};
use proptest::collection::vec;
use proptest::prelude::*;
use quiddity_core::{Asset, BlockId, Graph, Index, Version};
use quiddity_node::blobs::{Blobs, Hash};
use quiddity_node::gc::{Collected, Ledger, collect};
use quiddity_node::store::Store;

const DAY: SignedDuration = SignedDuration::from_hours(24);

fn t0() -> Timestamp {
    "2026-10-01T00:00:00Z".parse().unwrap()
}

/// A version that is later for a larger `n`. They all come from one graph,
/// because a version is a count of each peer's edits, so those of different
/// graphs are not comparable.
fn version_after(n: usize) -> Version {
    static VERSIONS: std::sync::OnceLock<Vec<Version>> = std::sync::OnceLock::new();
    VERSIONS.get_or_init(|| {
        let graph = Graph::new();
        let mut all = vec![graph.version()];
        for i in 0..4 {
            graph.create_block(None, None, &i.to_string()).unwrap();
            graph.commit();
            all.push(graph.version());
        }
        all
    })[n]
        .clone()
}

fn hashes(seeds: &[u8]) -> BTreeSet<Hash> {
    seeds.iter().map(|s| Hash::new([*s])).collect()
}

proptest! {
    /// What is written reads back, whatever the files, times and versions.
    #[test]
    fn the_ledger_reads_back_what_it_wrote(
        entries in vec((any::<u8>(), 0i64..4_000_000_000, 0usize..4), 0..8)
    ) {
        let mut ledger = Ledger::default();
        let mut all = BTreeSet::new();
        for (seed, seconds, n) in entries {
            all.insert(Hash::new([seed]));
            ledger.observe(&all, Timestamp::from_second(seconds).unwrap(), &version_after(n));
        }
        prop_assert_eq!(Ledger::from_text(&ledger.to_text()), ledger);
    }

    /// A file is due only if the grace has passed *and* the version it was
    /// seen at is one everybody has; neither alone is enough.
    #[test]
    fn a_file_is_due_only_with_the_grace_and_the_version(
        seen in 0usize..4,
        stored in 0usize..4,
        waited in 0i64..6,
    ) {
        let mut ledger = Ledger::default();
        ledger.observe(&hashes(&[1]), t0(), &version_after(seen));
        let due = ledger.due(t0() + DAY * waited as i32, DAY * 3, &version_after(stored));
        let expected = waited >= 3 && stored >= seen;
        prop_assert_eq!(!due.is_empty(), expected);
    }
}

#[test]
fn a_file_keeps_its_first_sighting_and_is_forgotten_when_referenced_again() {
    let mut ledger = Ledger::default();
    ledger.observe(&hashes(&[1, 2]), t0(), &version_after(1));
    let first = ledger.clone();
    assert!(!first.is_empty() && first.len() == 2);
    // Seen again later, at a later version: nothing about 1 and 2 changes.
    ledger.observe(&hashes(&[1, 2]), t0() + DAY, &version_after(3));
    assert_eq!(ledger, first);
    // 2 is referenced again, and 3 is new.
    ledger.observe(&hashes(&[1, 3]), t0() + DAY * 2, &version_after(2));
    assert_eq!(ledger.len(), 2);
    assert!(
        ledger
            .due(t0() + DAY * 2, DAY * 2, &version_after(3))
            .contains(&Hash::new([1]))
    );
    assert!(
        !ledger
            .due(t0() + DAY * 2, DAY * 2, &version_after(3))
            .contains(&Hash::new([3]))
    );
    // And 2 starts again from the beginning if it is dropped again.
    ledger.observe(&hashes(&[1, 2, 3]), t0() + DAY * 9, &version_after(3));
    assert!(
        !ledger
            .due(t0() + DAY * 9, DAY, &version_after(3))
            .contains(&Hash::new([2]))
    );

    ledger.forget(&hashes(&[1]));
    assert_eq!(ledger.len(), 2);
    assert!(Ledger::default().is_empty());
}

/// A line that can't be read is left out, which only starts that file's
/// clock again: the safe mistake.
#[test]
fn lines_that_cannot_be_read_are_left_out() {
    let mut ledger = Ledger::default();
    ledger.observe(&hashes(&[1]), t0(), &version_after(1));
    let good = ledger.to_text();
    let hash = Hash::new([9]);
    let text = format!(
        "{good}\nnot a hash 1 00\n{hash} soon 00\n{hash} 5\n{hash} 5 0\n{hash} 5 zz\n{hash} 5 00 extra\n"
    );
    assert_eq!(Ledger::from_text(&text), ledger);
    assert!(Ledger::from_text("").is_empty());
}

/// A small world: a graph with two other devices, two files with their
/// bytes, one of which is then dropped from the graph.
struct World {
    dir: tempfile::TempDir,
    graph: Graph,
    kept: Hash,
    dropped: Hash,
    dropped_entity: BlockId,
}

/// Adds a file to the graph: a blob block on a page of its own.
fn file(graph: &Graph, asset: &Asset) -> BlockId {
    let page = graph.create_block(None, None, "file").unwrap();
    graph.create_blob(page, asset).unwrap()
}

impl World {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let graph = Graph::new();
        let blobs = Blobs::open(dir.path()).await.unwrap();
        let mut entities = Vec::new();
        for bytes in [&b"kept bytes"[..], &b"dropped bytes"[..]] {
            let hash = blobs.put(bytes).await.unwrap();
            let asset =
                Asset::new(&hash.to_string(), "text/plain", bytes.len() as u64, None).unwrap();
            entities.push((hash, file(&graph, &asset)));
        }
        blobs.shutdown().await.unwrap();
        graph.commit();
        graph.record_stored("me", t0()).unwrap();
        graph.record_stored("other", t0()).unwrap();
        graph.delete_block(entities[1].1).unwrap();
        graph.commit();
        Self {
            dir,
            graph,
            kept: entities[0].0,
            dropped: entities[1].0,
            dropped_entity: entities[1].1,
        }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Both devices store the graph as it is now.
    fn everyone_stores(&self, when: Timestamp) {
        self.graph.record_stored("me", when).unwrap();
        self.graph.record_stored("other", when).unwrap();
    }

    async fn collect(&self, when: Timestamp, grace: SignedDuration, dry_run: bool) -> Collected {
        collect(self.path(), &self.graph, when, grace, dry_run)
            .await
            .unwrap()
    }

    async fn stored(&self) -> BTreeSet<Hash> {
        let blobs = Blobs::open(self.path()).await.unwrap();
        let stored = blobs.stored().await.unwrap();
        blobs.shutdown().await.unwrap();
        stored
    }
}

/// The whole rule, in order: the first run only notes; once the grace has
/// passed and everyone has stored what was seen, the file goes, and the file
/// the graph still refers to never does.
#[tokio::test]
async fn a_dropped_file_goes_once_the_grace_has_passed_and_everyone_has_seen_it_dropped() {
    let w = World::new().await;
    let first = w.collect(t0(), DAY, false).await;
    assert_eq!(
        first,
        Collected {
            in_use: 1,
            waiting: 1,
            removed: BTreeSet::new()
        }
    );
    assert_eq!(w.stored().await, BTreeSet::from([w.kept, w.dropped]));

    w.everyone_stores(t0() + DAY * 2);
    let second = w.collect(t0() + DAY * 2, DAY, false).await;
    assert_eq!(
        second,
        Collected {
            in_use: 1,
            waiting: 0,
            removed: BTreeSet::from([w.dropped])
        }
    );
    assert_eq!(w.stored().await, BTreeSet::from([w.kept]));

    // And again, with nothing left to do.
    let third = w.collect(t0() + DAY * 3, DAY, false).await;
    assert_eq!(
        third,
        Collected {
            in_use: 1,
            waiting: 0,
            removed: BTreeSet::new()
        }
    );
}

#[tokio::test]
async fn the_grace_period_is_waited_out_even_when_everyone_has_seen_it() {
    let w = World::new().await;
    w.collect(t0(), DAY * 3, false).await;
    w.everyone_stores(t0() + DAY);
    let early = w.collect(t0() + DAY * 2, DAY * 3, false).await;
    assert!(early.removed.is_empty());
    assert_eq!(w.stored().await.len(), 2);
}

/// A device that hasn't stored a version from after the file was dropped may
/// be about to refer to it: the file waits for that device, however long.
#[tokio::test]
async fn a_device_that_has_not_seen_it_dropped_holds_the_file() {
    let w = World::new().await;
    w.collect(t0(), DAY, false).await;
    // Only this device stores the graph; "other" is still at the old version.
    w.graph.record_stored("me", t0() + DAY * 2).unwrap();
    let held = w.collect(t0() + DAY * 2, DAY, false).await;
    assert_eq!(
        held,
        Collected {
            in_use: 1,
            waiting: 1,
            removed: BTreeSet::new()
        }
    );
    assert_eq!(w.stored().await.len(), 2);

    // It catches up, and then the file may go.
    w.everyone_stores(t0() + DAY * 3);
    let released = w.collect(t0() + DAY * 3, DAY, false).await;
    assert_eq!(released.removed, BTreeSet::from([w.dropped]));
}

/// When this replica lacks something another device has stored, the rule
/// can't be applied, and nothing is deleted.
#[tokio::test]
async fn nothing_goes_while_another_device_has_stored_what_this_replica_lacks() {
    let w = World::new().await;
    w.collect(t0(), DAY, false).await;
    w.everyone_stores(t0() + DAY * 2);
    w.graph
        .vouch_for("ahead", &version_after(4), t0() + DAY * 2)
        .unwrap();
    let report = w.collect(t0() + DAY * 3, DAY, false).await;
    assert_eq!(
        report,
        Collected {
            in_use: 1,
            waiting: 1,
            removed: BTreeSet::new()
        }
    );
    assert_eq!(w.stored().await.len(), 2);
}

/// Referring to a file again before it is due starts its clock over.
#[tokio::test]
async fn a_file_referred_to_again_is_kept_and_forgotten() {
    let w = World::new().await;
    w.collect(t0(), DAY, false).await;
    let asset = Asset::new(&w.dropped.to_string(), "text/plain", 13, None).unwrap();
    file(&w.graph, &asset);
    w.graph.commit();
    w.everyone_stores(t0() + DAY * 2);
    let report = w.collect(t0() + DAY * 2, DAY, false).await;
    assert_eq!(
        report,
        Collected {
            in_use: 2,
            waiting: 0,
            removed: BTreeSet::new()
        }
    );
    assert_eq!(w.stored().await.len(), 2);
    assert!(Ledger::load(w.path()).unwrap().is_empty());
}

/// A dry run says what would happen and changes nothing, not even the ledger.
#[tokio::test]
async fn a_dry_run_changes_nothing() {
    let w = World::new().await;
    let ledger = w.path().join("unreferenced-blobs.txt");
    let preview = w.collect(t0(), DAY, true).await;
    assert_eq!(preview.waiting, 1);
    assert!(!ledger.exists(), "no sighting recorded");

    w.collect(t0(), DAY, false).await;
    let recorded = fs::read_to_string(&ledger).unwrap();
    w.everyone_stores(t0() + DAY * 2);
    let preview = w.collect(t0() + DAY * 2, DAY, true).await;
    assert_eq!(preview.removed, BTreeSet::from([w.dropped]));
    assert_eq!(w.stored().await.len(), 2, "nothing deleted");
    assert_eq!(fs::read_to_string(&ledger).unwrap(), recorded);
}

/// A line that can't be read whole still names the file it began with: a
/// damaged line is a reason to look, not to delete.
#[tokio::test]
async fn a_damaged_blob_line_still_protects_its_file() {
    let w = World::new().await;
    let entity = w.graph.create_block(None, None, "damaged").unwrap();
    w.graph
        .create_block(
            Some(entity),
            None,
            &format!("##blob({}, nonsense)", w.dropped),
        )
        .unwrap();
    w.graph.commit();
    assert!(Index::build(&w.graph).unwrap().assets().count() == 1);
    w.collect(t0(), DAY, false).await;
    w.everyone_stores(t0() + DAY * 2);
    let report = w.collect(t0() + DAY * 2, DAY, false).await;
    assert_eq!(
        report,
        Collected {
            in_use: 2,
            waiting: 0,
            removed: BTreeSet::new()
        }
    );
    let _ = w.dropped_entity;
}

/// The hash in a damaged line protects its file however it is written. `Hash`
/// reads only lower-case hex, and a `##blob` line may have it in capitals.
#[tokio::test]
async fn a_damaged_line_with_a_capitalised_hash_still_protects_its_file() {
    let w = World::new().await;
    let entity = w.graph.create_block(None, None, "damaged").unwrap();
    let shouting = w.dropped.to_string().to_uppercase();
    w.graph
        .create_block(Some(entity), None, &format!("##blob({shouting}, nonsense)"))
        .unwrap();
    w.graph.commit();
    w.collect(t0(), DAY, false).await;
    w.everyone_stores(t0() + DAY * 2);
    let report = w.collect(t0() + DAY * 2, DAY, false).await;
    assert_eq!(
        report,
        Collected {
            in_use: 2,
            waiting: 0,
            removed: BTreeSet::new()
        }
    );
    assert_eq!(w.stored().await.len(), 2);
}

fn node(graph: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_quiddity-node"));
    command.arg("--graph").arg(graph);
    command
}

fn run(command: &mut Command) -> String {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// With the real command: a file nothing refers to is noted, and then, with no
/// grace, deleted on the next run, while a file the graph refers to stays.
#[tokio::test]
async fn the_command_notes_and_then_deletes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph");
    let store = Store::new(&path);
    let graph = store.init(false).unwrap();
    let blobs = Blobs::open(&path).await.unwrap();
    let kept = blobs.put(&b"in use"[..]).await.unwrap();
    let loose = blobs.put(&b"loose"[..]).await.unwrap();
    let asset = Asset::new(&kept.to_string(), "text/plain", 6, Some("in use.txt")).unwrap();
    file(&graph, &asset);
    graph.commit();
    store.save(&graph).unwrap();
    blobs.shutdown().await.unwrap();

    let dry = run(node(&path).args(["gc", "--dry-run", "--grace-days", "0"]));
    assert_eq!(
        dry,
        "1 files in use\n1 unreferenced, not yet due\nwould delete 0 files"
    );
    let first = run(node(&path).args(["gc", "--grace-days", "0"]));
    assert_eq!(
        first,
        "1 files in use\n1 unreferenced, not yet due\ndeleted 0 files"
    );
    let second = run(node(&path).args(["gc", "--grace-days", "0"]));
    assert_eq!(
        second,
        "1 files in use\n0 unreferenced, not yet due\ndeleted 1 files"
    );

    let blobs = Blobs::open(&path).await.unwrap();
    assert!(blobs.has(kept).await.unwrap());
    assert!(!blobs.has(loose).await.unwrap());
    blobs.shutdown().await.unwrap();
}

#[test]
fn a_blank_replica_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph");
    run(node(&path).args(["init", "--blank"]));
    let output = node(&path).arg("gc").output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("blank replica"));
}
