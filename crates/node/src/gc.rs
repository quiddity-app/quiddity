//! Collecting the files nothing refers to any more.
//!
//! A file may be deleted only when it is unreferenced now; has been seen
//! unreferenced for the whole grace period; and was first seen unreferenced at
//! a version of the graph that every active device has since stored. The last
//! is what keeps a file whose last reference was removed on one device from
//! going while another, that has not heard of the removal, is making a new
//! reference to it: either that device has stored what we saw, and so knew,
//! or it is not yet counted as having, and the file waits.
//!
//! When the rule cannot be applied, nothing is deleted: no compaction point
//! (this replica lacks something another device has stored), a ledger that
//! cannot be read (the clocks start again), and a graph that has never joined
//! one.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;

use rusqlite::params;

use jiff::{SignedDuration, Timestamp};
use quiddity_core::{Graph, Index, Version};

use crate::blobs::{BlobError, Blobs, Hash};
use crate::store::{ACTIVE, Store};

/// How long a file must have gone unreferenced before it may be deleted: as
/// long as a device may be silent before it stops holding back compaction.
pub const GRACE: SignedDuration = ACTIVE;

/// Where the ledger was kept, before it was kept in the graph's file: a text
/// file in the graph's directory, read once and set aside. (Not in `blobs/`,
/// which belonged to the store.)
const LEDGER: &str = "unreferenced-blobs.txt";

#[derive(Debug, thiserror::Error)]
pub enum CollectError {
    #[error(transparent)]
    Blob(#[from] BlobError),
    #[error(transparent)]
    Graph(#[from] quiddity_core::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Store(#[from] crate::StoreError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Seen {
    at: Timestamp,
    version: Version,
}

/// When each file that nothing refers to was first seen so, and at which
/// version of the graph. Kept between runs.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Ledger {
    seen: BTreeMap<Hash, Seen>,
}

impl Ledger {
    /// Notes which files are unreferenced as of `now`, at `version`. A file
    /// already waiting keeps its first sighting; one that is referenced again
    /// is forgotten, so its clock starts again if it is ever dropped.
    pub fn observe(&mut self, unreferenced: &BTreeSet<Hash>, now: Timestamp, version: &Version) {
        self.seen.retain(|hash, _| unreferenced.contains(hash));
        for hash in unreferenced {
            self.seen.entry(*hash).or_insert_with(|| Seen {
                at: now,
                version: version.clone(),
            });
        }
    }

    /// The files that may go: first seen unreferenced at least `grace` ago, at
    /// a version `stored_by_all` includes.
    pub fn due(
        &self,
        now: Timestamp,
        grace: SignedDuration,
        stored_by_all: &Version,
    ) -> BTreeSet<Hash> {
        self.seen
            .iter()
            .filter(|(_, s)| {
                now.duration_since(s.at) >= grace && stored_by_all.includes(&s.version)
            })
            .map(|(hash, _)| *hash)
            .collect()
    }

    /// Lets go of files that have been dealt with.
    pub fn forget(&mut self, hashes: &BTreeSet<Hash>) {
        self.seen.retain(|hash, _| !hashes.contains(hash));
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    /// Reads the text ledger that was kept before the file held it: one file a
    /// line, its hash, when it was first seen unreferenced (in seconds since
    /// 1970) and the graph's version then (in hex). A line it cannot read is
    /// left out, which only starts that file's clock again: waiting longer is
    /// the safe mistake.
    pub fn from_text(text: &str) -> Self {
        let seen = text
            .lines()
            .filter_map(|line| {
                let mut parts = line.split(' ');
                let hash = parts.next()?.parse::<Hash>().ok()?;
                let at = Timestamp::from_second(parts.next()?.parse().ok()?).ok()?;
                let version = Version::decode(&from_hex(parts.next()?)?).ok()?;
                parts
                    .next()
                    .is_none()
                    .then_some((hash, Seen { at, version }))
            })
            .collect();
        Self { seen }
    }

    /// The ledger in the graph's file. A text ledger left from before is
    /// taken in first, and set aside.
    pub fn load(graph_dir: &Path) -> Result<Self, CollectError> {
        let store = Store::new(graph_dir);
        let db = store.connection()?;
        // Only a graph in a directory can have had the text file beside it.
        if let Some(dir) = store.legacy_dir() {
            take_in_text_ledger(&db, dir)?;
        }
        let rows: Vec<(Vec<u8>, i64, Vec<u8>)> = db
            .prepare("SELECT hash, seen, version FROM unreferenced")
            .and_then(|mut rows| {
                rows.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                    .collect()
            })
            .map_err(crate::StoreError::from)?;
        // A row it cannot read is left out, as a line of the text was.
        let seen = rows
            .into_iter()
            .filter_map(|(hash, at, version)| {
                let hash = Hash::from_bytes(<[u8; 32]>::try_from(hash).ok()?);
                let at = Timestamp::from_second(at).ok()?;
                let version = Version::decode(&version).ok()?;
                Some((hash, Seen { at, version }))
            })
            .collect();
        Ok(Self { seen })
    }

    /// Saves in one transaction, as the graph's snapshot is, replacing what
    /// was kept.
    pub fn save(&self, graph_dir: &Path) -> Result<(), CollectError> {
        let mut db = Store::new(graph_dir).connection()?;
        let saved = (|| {
            let tx = db.transaction()?;
            tx.execute("DELETE FROM unreferenced", [])?;
            for (hash, seen) in &self.seen {
                tx.execute(
                    "INSERT INTO unreferenced (hash, seen, version) VALUES (?1, ?2, ?3)",
                    params![
                        hash.as_bytes().as_slice(),
                        seen.at.as_second(),
                        seen.version.encode()
                    ],
                )?;
            }
            tx.commit()
        })();
        Ok(saved.map_err(crate::StoreError::from)?)
    }
}

/// Takes in the text ledger kept before the file held it, if there is one, and
/// sets it aside. Sightings already in the file stand: they are the later ones.
fn take_in_text_ledger(db: &rusqlite::Connection, dir: &Path) -> Result<(), CollectError> {
    let legacy = dir.join(LEDGER);
    match std::fs::read_to_string(&legacy) {
        Ok(text) => {
            for (hash, seen) in Ledger::from_text(&text).seen {
                db.execute(
                    "INSERT OR IGNORE INTO unreferenced (hash, seen, version) VALUES (?1, ?2, ?3)",
                    params![
                        hash.as_bytes().as_slice(),
                        seen.at.as_second(),
                        seen.version.encode()
                    ],
                )
                .map_err(crate::StoreError::from)?;
            }
            std::fs::rename(&legacy, dir.join(format!("{LEDGER}.migrated")))?;
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

/// What a collection found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Collected {
    /// Stored files the graph refers to.
    pub in_use: usize,
    /// Stored files it doesn't, that are not yet due.
    pub waiting: usize,
    /// The files deleted, or with `dry_run`, that would be.
    pub removed: BTreeSet<Hash>,
}

/// The hashes the graph refers to. A `##blob` line that can't be read as a
/// whole still counts for the hash it begins with: a damaged line is a reason
/// to look, not to delete.
fn referenced(index: &Index) -> BTreeSet<Hash> {
    let mut hashes = BTreeSet::new();
    for parameter in index.blob_lines() {
        let first = parameter.split(',').next().unwrap_or("");
        // `Hash` reads lower-case hex only, and a 64-character name only as
        // hex, so a line written in capitals is lowered first, and a name of
        // another length, which it would try as base32, is not tried.
        let first = first.trim().to_ascii_lowercase();
        if first.len() == 64
            && let Ok(hash) = first.parse::<Hash>()
        {
            hashes.insert(hash);
        }
    }
    hashes
}

/// Deletes the stored files nothing refers to any more, by the rule in the
/// module's description, and keeps the ledger. With `dry_run` it only says what
/// it would do, and keeps nothing.
///
/// The graph's file is opened here and the graph is saved by the caller, so a
/// serving node must be stopped first: what it saved since the graph was loaded
/// would be written over. Nothing enforces that.
pub async fn collect(
    graph_dir: &Path,
    graph: &Graph,
    now: Timestamp,
    grace: SignedDuration,
    dry_run: bool,
) -> Result<Collected, CollectError> {
    let blobs = Blobs::open(graph_dir).await?;
    let stored = blobs.stored().await?;
    let referenced = referenced(&Index::build(graph)?);
    let unreferenced: BTreeSet<Hash> = stored.difference(&referenced).copied().collect();

    let mut ledger = Ledger::load(graph_dir)?;
    ledger.observe(&unreferenced, now, &graph.version());
    let due = match graph.compaction_point(now, ACTIVE) {
        Some(point) => ledger.due(now, grace, &point),
        None => BTreeSet::new(),
    };
    // Whatever else is true, nothing referenced, and nothing not stored, goes.
    let due: BTreeSet<Hash> = due.intersection(&unreferenced).copied().collect();

    let report = Collected {
        in_use: stored.len() - unreferenced.len(),
        waiting: unreferenced.len() - due.len(),
        removed: due.clone(),
    };
    if dry_run {
        blobs.shutdown().await?;
        return Ok(report);
    }
    // The sightings first, so they are kept even if deleting fails.
    ledger.save(graph_dir)?;
    blobs.delete(&due).await?;
    blobs.shutdown().await?;
    ledger.forget(&due);
    ledger.save(graph_dir)?;
    Ok(report)
}
