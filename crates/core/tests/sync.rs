//! Replicas editing concurrently and syncing in arbitrary orders.
#![expect(clippy::unwrap_used, reason = "a test failing to set up should panic")]

mod common;

use common::{Op, apply, layout, op, sync, sync_all};
use jiff::{SignedDuration, Timestamp};
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::Index;
use quiddity_core::{Graph, SCHEMA_VERSION, Version};

const REPLICAS: usize = 3;
const ACTIVE: SignedDuration = SignedDuration::from_hours(24 * 30);

fn now() -> Timestamp {
    "2026-10-26T12:00:00Z".parse().unwrap()
}

#[derive(Debug, Clone)]
enum Step {
    Edit(usize, Op),
    Sync(usize, usize),
    /// The device restarts, reloading from its own snapshot.
    Restart(usize),
    /// The device records that it has stored what it has.
    Store(usize),
    /// The device drops the history every device has stored, if its
    /// compaction point says that's safe.
    Compact(usize),
}

impl Step {
    /// The replica a step acts on alone.
    fn replica(&self) -> Option<usize> {
        match *self {
            Step::Edit(r, _) | Step::Restart(r) | Step::Store(r) | Step::Compact(r) => Some(r),
            Step::Sync(..) => None,
        }
    }
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        6 => (0..REPLICAS, op()).prop_map(|(r, op)| Step::Edit(r, op)),
        2 => (0..REPLICAS, 0..REPLICAS).prop_map(|(a, b)| Step::Sync(a, b)),
        1 => (0..REPLICAS).prop_map(Step::Restart),
        2 => (0..REPLICAS).prop_map(Step::Store),
        2 => (0..REPLICAS).prop_map(Step::Compact),
    ]
}

/// One replica that made the graph, one that joined from its snapshot, and
/// one that started blank and only ever receives updates.
fn replicas() -> Vec<Graph> {
    // The device that makes a graph records itself from the start, and
    // vouches for each device it hands the graph to.
    let origin = Graph::new();
    origin.record_stored("0", now()).unwrap();
    origin.vouch_for("1", &origin.version(), now()).unwrap();
    let joined = Graph::from_snapshot(&origin.snapshot().unwrap()).unwrap();
    vec![origin, joined, Graph::blank()]
}

/// Syncs two of the replicas. Each that has something to give vouches for
/// the other first, as a device does for a peer it hasn't heard of.
fn meet(replicas: &mut [Graph], a: usize, b: usize) -> Result<(), TestCaseError> {
    for (x, y) in [(a, b), (b, a)] {
        if !blank(&replicas[x]) {
            let theirs = replicas[y].version();
            replicas[x]
                .vouch_for(&y.to_string(), &theirs, now())
                .unwrap();
        }
    }
    bring_up_to_date(replicas, a, b)?;
    bring_up_to_date(replicas, b, a)
}

/// Sends updates, or, to a blank replica behind `from`'s history, a
/// snapshot to start from. Compacting by the device list means no other
/// replica is ever behind.
fn bring_up_to_date(replicas: &mut [Graph], from: usize, to: usize) -> Result<(), TestCaseError> {
    if replicas[from].can_update(&replicas[to].version()) {
        return send(&replicas[from], &replicas[to]);
    }
    prop_assert!(blank(&replicas[to]), "only a blank replica is ever behind");
    replicas[to] = Graph::from_snapshot(&replicas[from].snapshot().unwrap()).unwrap();
    Ok(())
}

/// A replica that has yet to join: it has nothing to edit or store.
fn blank(graph: &Graph) -> bool {
    graph.version() == Version::default()
}

/// Edits that only add: no deletions and no rewrites of text, so nothing
/// any replica does should be lost.
#[derive(Debug, Clone)]
enum Addition {
    Create {
        replica: usize,
        parent: Option<Index>,
    },
    Append {
        replica: usize,
        block: Index,
    },
    Move {
        replica: usize,
        block: Index,
        parent: Option<Index>,
    },
    Sync(usize, usize),
}

fn addition() -> impl Strategy<Value = Addition> {
    let parent = proptest::option::weighted(0.8, any::<Index>());
    prop_oneof![
        3 => (0..REPLICAS, parent.clone()).prop_map(|(replica, parent)| Addition::Create { replica, parent }),
        3 => (0..REPLICAS, any::<Index>()).prop_map(|(replica, block)| Addition::Append { replica, block }),
        2 => (0..REPLICAS, any::<Index>(), parent).prop_map(|(replica, block, parent)| Addition::Move { replica, block, parent }),
        2 => (0..REPLICAS, 0..REPLICAS).prop_map(|(a, b)| Addition::Sync(a, b)),
    ]
}

/// Sends `to` what it is missing from `from`, checking that versions say
/// what each replica has: the protocol relies on them to send no more and
/// no less than needed.
fn send(from: &Graph, to: &Graph) -> Result<(), TestCaseError> {
    let before = to.version();
    prop_assert_eq!(&Version::decode(&before.encode()).unwrap(), &before);
    let behind = !before.includes(&from.version());

    let updates = from.updates_since(&before).unwrap();
    to.import(&updates).unwrap();
    let after = to.version();
    let mut expected = before.clone();
    expected.merge(&Graph::version_of_updates(&updates).unwrap());
    prop_assert_eq!(&after, &expected, "the updates say what they bring");
    prop_assert!(after.includes(&from.version()));
    prop_assert_eq!(behind, after != before, "only news changes the version");
    Ok(())
}

/// A replica compacted past what another has seen can't update it, and
/// says so, rather than sending updates that would never apply. A blank
/// replica can still start from its snapshot.
#[test]
fn compaction_leaves_behind_only_those_it_must() {
    let (a, behind) = (Graph::new(), Graph::blank());
    sync(&a, &behind).unwrap();
    for i in 0..3 {
        a.create_block(None, None, &i.to_string()).unwrap();
        a.commit();
    }
    let compacted = Graph::from_snapshot(&a.shallow_snapshot(&a.version()).unwrap()).unwrap();
    assert!(!compacted.can_update(&behind.version()));
    assert!(matches!(
        compacted.updates_since(&behind.version()),
        Err(quiddity_core::Error::BeforeHistory)
    ));
    assert!(!compacted.can_update(&Graph::blank().version()));
    // Its snapshot says where it starts, as updates say where they do.
    assert_eq!(
        Graph::start_of(&compacted.snapshot().unwrap()).unwrap(),
        compacted.history_start()
    );
    assert_eq!(
        Graph::start_of(&a.snapshot().unwrap()).unwrap(),
        Version::default()
    );
    let updates = a.updates_since(&behind.version()).unwrap();
    assert!(
        behind
            .version()
            .includes(&Graph::start_of(&updates).unwrap())
    );
    assert!(
        !Graph::blank()
            .version()
            .includes(&Graph::start_of(&updates).unwrap())
    );
    let joined = Graph::from_snapshot(&compacted.snapshot().unwrap()).unwrap();
    assert_eq!(layout(&joined), layout(&a));
    // And from then on it syncs as usual.
    joined.create_block(None, None, "later").unwrap();
    sync(&joined, &a).unwrap();
    assert_eq!(layout(&joined), layout(&a));
}

/// Compacting where there's nothing more to drop changes nothing, even
/// right at the start of a graph's history.
#[test]
fn compacting_again_drops_nothing_more() {
    let graph = Graph::new();
    let compacted =
        Graph::from_snapshot(&graph.shallow_snapshot(&graph.version()).unwrap()).unwrap();
    for at in [Version::default(), compacted.version()] {
        let again = Graph::from_snapshot(&compacted.shallow_snapshot(&at).unwrap()).unwrap();
        assert_eq!(again.history_start(), compacted.history_start());
        assert_eq!(again.schema_version(), Some(SCHEMA_VERSION));
    }
}

/// The point of compacting: a much-edited block's history goes.
#[test]
fn compaction_drops_history() {
    let graph = Graph::new();
    let block = graph.create_block(None, None, "").unwrap();
    for i in 0..300 {
        graph
            .set_text(block, &format!("draft {i} ").repeat(10))
            .unwrap();
        graph.commit();
    }
    let full = graph.snapshot().unwrap().len();
    let shallow = graph.shallow_snapshot(&graph.version()).unwrap().len();
    assert!(shallow * 4 < full, "{shallow} of {full} bytes");
}

/// A blank replica saved and loaded again can still start from a compacted
/// replica's snapshot, and nothing is ever silently left unapplied.
#[test]
fn imports_apply_or_say_why_not() {
    let a = Graph::new();
    let before = a.version();
    a.create_block(None, None, "hello").unwrap();
    a.commit();
    let compacted = a.shallow_snapshot(&a.version()).unwrap();

    let reloaded = Graph::from_snapshot(&Graph::blank().snapshot().unwrap()).unwrap();
    reloaded.import(&compacted).unwrap();
    assert_eq!(layout(&reloaded), layout(&a));

    // Updates made on history the blank replica hasn't seen.
    let updates = a.updates_since(&before).unwrap();
    assert!(matches!(
        Graph::blank().import(&updates),
        Err(quiddity_core::Error::NotApplied)
    ));
}

/// Edits made on history a replica has dropped can't be applied, and that
/// is reported as such: the replica needs that history back first.
#[test]
fn edits_on_dropped_history_are_refused() {
    let (a, y) = (Graph::new(), Graph::blank());
    sync(&a, &y).unwrap();
    // y writes without having seen a's next edit.
    y.create_block(None, None, "from y").unwrap();
    y.commit();
    a.create_block(None, None, "from a").unwrap();
    a.commit();
    let compacted = Graph::from_snapshot(&a.shallow_snapshot(&a.version()).unwrap()).unwrap();
    let updates = y.updates_since(&a.version().meet(&y.version())).unwrap();
    assert!(matches!(
        compacted.import(&updates),
        Err(quiddity_core::Error::BeforeHistory)
    ));
}

#[test]
fn a_blank_replica_learns_the_schema_by_syncing() {
    let (origin, blank) = (Graph::new(), Graph::blank());
    assert_eq!(blank.schema_version(), None);
    sync(&origin, &blank).unwrap();
    assert_eq!(blank.schema_version(), Some(SCHEMA_VERSION));
}

proptest! {
    #![proptest_config(common::config(64))]

    /// However edits, syncs and compaction interleave, once everyone has
    /// synced all replicas hold the same graph, and a snapshot reproduces it.
    /// Compacting only where `compaction_point` allows never leaves a replica
    /// unable to send or receive: `send` would fail.
    #[test]
    fn replicas_converge(steps in vec(step(), 1..60)) {
        let mut replicas = replicas();
        for step in &steps {
            match *step {
                _ if step.replica().is_some_and(|r| blank(&replicas[r])) => {}
                Step::Edit(r, ref op) => apply(&replicas[r], op),
                Step::Sync(a, b) => meet(&mut replicas, a, b)?,
                Step::Restart(r) => {
                    replicas[r] = Graph::from_snapshot(&replicas[r].snapshot().unwrap()).unwrap();
                }
                Step::Store(r) => replicas[r].record_stored(&r.to_string(), now()).unwrap(),
                Step::Compact(r) => {
                    let Some(at) = replicas[r].compaction_point(now(), ACTIVE) else { continue };
                    let before = layout(&replicas[r]);
                    replicas[r] = Graph::from_snapshot(&replicas[r].shallow_snapshot(&at).unwrap()).unwrap();
                    prop_assert_eq!(layout(&replicas[r]), before);
                }
            }
        }
        for _ in 0..2 {
            for (a, b) in [(0, 1), (0, 2), (1, 2)] {
                meet(&mut replicas, a, b)?;
            }
        }

        let expected = layout(&replicas[0]);
        for replica in &replicas {
            prop_assert_eq!(replica.schema_version(), Some(SCHEMA_VERSION));
            prop_assert_eq!(&layout(replica), &expected);
            let reloaded = Graph::from_snapshot(&replica.snapshot().unwrap()).unwrap();
            prop_assert_eq!(&layout(&reloaded), &expected);
        }
    }

    /// Without deletions or rewrites, every block any replica created and
    /// every piece of text any replica appended survives, wherever
    /// concurrent moves put it.
    #[test]
    fn concurrent_additions_survive(steps in vec(addition(), 1..60)) {
        let replicas = replicas();
        let mut created = 0;
        let mut appended: Vec<(quiddity_core::BlockId, String)> = Vec::new();
        for step in &steps {
            let pick = |graph: &Graph, i: &Index| {
                let blocks = graph.blocks();
                (!blocks.is_empty()).then(|| blocks[i.index(blocks.len())])
            };
            match step {
                Addition::Create { replica, parent } => {
                    let graph = &replicas[*replica];
                    let parent = parent.as_ref().and_then(|p| pick(graph, p));
                    graph.create_block(parent, None, &format!("<{created}>")).unwrap();
                    created += 1;
                }
                Addition::Append { replica, block } => {
                    let graph = &replicas[*replica];
                    let Some(block) = pick(graph, block) else { continue };
                    let token = format!("+{};", appended.len());
                    graph.set_text(block, &format!("{}{token}", graph.text(block).unwrap())).unwrap();
                    appended.push((block, token));
                }
                Addition::Move { replica, block, parent } => {
                    let graph = &replicas[*replica];
                    let (Some(block), parent) = (pick(graph, block), parent.as_ref().and_then(|p| pick(graph, p))) else { continue };
                    // A move under its own descendant is refused locally.
                    drop(graph.move_block(block, parent, None));
                }
                Addition::Sync(a, b) => sync(&replicas[*a], &replicas[*b]).unwrap(),
            }
            replicas.iter().for_each(Graph::commit);
        }
        sync_all(&replicas).unwrap();

        // Each block's text is its creation token followed by everything
        // appended to it, in some order, each exactly once.
        let expected: Vec<(String, Vec<&str>)> = replicas[0]
            .blocks()
            .into_iter()
            .map(|b| {
                let mine = appended.iter().filter(|(on, _)| *on == b).map(|(_, t)| t.as_str()).collect();
                (replicas[0].text(b).unwrap(), mine)
            })
            .collect();
        prop_assert_eq!(expected.len(), created, "every created block survives");
        for (text, tokens) in &expected {
            let origin = &text[..=text.find('>').unwrap()];
            let mut rest = text[origin.len()..].to_owned();
            for token in tokens {
                prop_assert_eq!(rest.matches(token).count(), 1, "{:?} in {:?}", token, text);
                rest = rest.replacen(token, "", 1);
            }
            prop_assert_eq!(rest, "", "{:?} has unexpected text", text);
        }
        for replica in &replicas[1..] {
            prop_assert_eq!(layout(replica), layout(&replicas[0]));
        }
    }
}
