//! A graph on disk: one SQLite file holding the graph's snapshot and the
//! device's endpoint key.
//!
//! SQLite is the file format because it is one file that other tools can
//! open, it commits atomically, and more of what a device keeps (the bytes
//! of binaries, the update log) can join it as tables. See
//! <https://sqlite.org/appfileformat.html>.
//!
//! **The file is treated as untrusted input**, because a graph may arrive by
//! copy from somewhere else (<https://sqlite.org/security.html>): it must carry
//! our `application_id` and a schema version we know, its schema must be
//! exactly ours (no triggers, views or tables we didn't make), and the
//! connection runs with defensive mode on and with triggers, views and
//! schema-defined functions off. The file holds the secret key, so it is
//! created readable by its owner only.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use iroh::SecretKey;
use jiff::{SignedDuration, Timestamp};
use quiddity_core::{Graph, Version};
use rusqlite::config::DbConfig;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use tracing::{debug, info, instrument, warn};

use crate::StoreError;

/// The graph, inside the graph's directory.
const FILE: &str = "graph.quiddity";
/// What a directory held before the graph was one file: the snapshot, and the
/// device's secret key. Read once, to make [`FILE`], and set aside.
const LEGACY_SNAPSHOT: &str = "graph.loro";
const LEGACY_KEY: &str = "device.key";
const LEGACY_BACKUP: &str = "graph.loro.bak";
const SET_ASIDE: &str = "migrated";

/// Makes the names a file is built under unique within the process; the
/// process ID covers other processes sharing a directory.
static STAGING: AtomicU64 = AtomicU64::new(0);

/// Flushes a directory's entries, so a rename or a new link survives power
/// loss. Windows can't open a directory to do so, and doesn't need it to.
fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    fs::File::open(dir)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

/// SQLite's `application_id`, "QDDY": says what the file is for, so that
/// `file(1)` can tell and another application's database is refused.
/// The rows of the `snapshot` table.
const CURRENT: i64 = 1;
const PREVIOUS: i64 = 2;

const APPLICATION_ID: i32 = 0x5144_4459;
/// SQLite's `user_version`: the layout of the tables below.
const LAYOUT: i32 = 2;

/// Exactly the schema, as SQLite stores it (its `sql` column is the statement
/// as written), so that a file with anything else in it is refused. One row
/// each, enforced by the key, so there is no question which is current.
/// The snapshot has two: row 1 is the current one, and row 2 the one before
/// it, which `load` falls back to if row 1 can't be decoded.
const TABLES: [(&str, &str); 3] = [
    // Complete blobs by BLAKE3 hash. `outboard` (the hashes iroh-blobs needs
    // to prove parts of the blob) comes before `data` so that reading it
    // never walks the pages `data` overflows into.
    (
        "blob",
        "CREATE TABLE blob (id INTEGER PRIMARY KEY, hash BLOB NOT NULL UNIQUE CHECK (length(hash) = 32), size INTEGER NOT NULL CHECK (size >= 0), outboard BLOB NOT NULL, data BLOB NOT NULL CHECK (length(data) = size)) STRICT",
    ),
    (
        "snapshot",
        "CREATE TABLE snapshot (id INTEGER PRIMARY KEY CHECK (id IN (1, 2)), data BLOB NOT NULL) STRICT",
    ),
    (
        "endpoint_key",
        "CREATE TABLE endpoint_key (id INTEGER PRIMARY KEY CHECK (id = 1), secret BLOB NOT NULL CHECK (length(secret) = 32)) STRICT",
    ),
];

/// How long to wait for another process that has the file open before saying
/// so: a node that is serving, and a command run beside it.
#[cfg(not(test))]
const BUSY: Duration = Duration::from_secs(5);
/// Short in tests, which make a save wait for a lock that is never let go.
#[cfg(test)]
const BUSY: Duration = Duration::from_millis(100);

/// How long a device can go without saving before it stops holding back
/// other devices' compaction.
pub const ACTIVE: SignedDuration = SignedDuration::from_hours(24 * 30);

#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
    /// Serialises saves, which come from the command thread and the
    /// save-on-change task at once, and holds the version the snapshot in the
    /// file is known to hold: what a save may say this device has stored.
    /// Clones share it.
    durable: Arc<Mutex<Version>>,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            durable: Arc::default(),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file(&self) -> PathBuf {
        self.dir.join(FILE)
    }

    /// Creates the directory with an empty graph (or, with `blank`, an empty
    /// replica for a device that will sync an existing graph). A file that
    /// holds only the device's key, because something asked for the key first
    /// or a graph was set aside, is filled.
    #[instrument(level = "debug", skip_all, fields(dir = %self.dir.display()))]
    pub fn init(&self, blank: bool) -> Result<Graph, StoreError> {
        if self.holds_graph() {
            return Err(StoreError::Exists(self.dir.clone()));
        }
        self.ensure()?;
        let graph = if blank { Graph::blank() } else { Graph::new() };
        self.save(&graph)?;
        Ok(graph)
    }

    /// Whether there is a graph here, readable or not: a file that can't be
    /// opened is somebody's graph, and not for `init` to replace.
    fn holds_graph(&self) -> bool {
        if self.dir.join(LEGACY_SNAPSHOT).exists() {
            return true;
        }
        if !self.file().exists() {
            return false;
        }
        match self.open() {
            Ok(db) => db
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM snapshot WHERE id = 1)",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(true),
            Err(_) => true,
        }
    }

    #[instrument(level = "debug", skip_all, fields(dir = %self.dir.display()))]
    pub fn load(&self) -> Result<Graph, StoreError> {
        let db = self.open()?;
        // The cheap checks ran on opening; this reads every page, which is
        // what a file from elsewhere deserves once, on being read.
        let verdict: String = db.pragma_query_value(None, "quick_check", |r| r.get(0))?;
        if verdict != "ok" {
            return Err(self.not_ours(format!("it is damaged: {verdict}")));
        }
        let generation = |id: i64| -> Result<Option<Vec<u8>>, StoreError> {
            Ok(db
                .query_row("SELECT data FROM snapshot WHERE id = ?1", [id], |r| {
                    r.get(0)
                })
                .optional()?)
        };
        // A file with a key and no graph is a device that has none yet.
        let bytes = generation(CURRENT)?.ok_or_else(|| StoreError::Missing(self.dir.clone()))?;
        debug!(bytes = bytes.len(), "loading the snapshot");
        let mut durable = self.durable();
        match Graph::from_snapshot(&bytes) {
            Ok(graph) => {
                *durable = graph.version();
                Ok(graph)
            }
            Err(e) => {
                // A snapshot that can't be decoded is never replaced by an
                // empty graph, but the previous generation is better than
                // refusing to start. It may be behind what peers know this
                // device stored: they will say so.
                let Some(previous) = generation(PREVIOUS)? else {
                    return Err(e.into());
                };
                let graph = Graph::from_snapshot(&previous).map_err(|_| e)?;
                warn!("the snapshot couldn't be loaded: using the previous one");
                *durable = graph.version();
                Ok(graph)
            }
        }
    }

    fn durable(&self) -> MutexGuard<'_, Version> {
        self.durable.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Saves in one transaction, and durably: a crash mid-save, even a power
    /// loss, leaves the previous snapshot, which is also kept as the
    /// previous generation.
    ///
    /// The saved graph records that this device has stored what the snapshot
    /// held before this save, and keeps only the history some device might
    /// still need. Recording what is being written instead would tell other
    /// devices, which act on it at once, about a version this device might
    /// never have if the write fails. The record lags one save, then; the
    /// history kept doesn't. A blank replica records nothing: it has yet to
    /// join.
    #[instrument(level = "debug", skip_all, fields(dir = %self.dir.display()))]
    pub fn save(&self, graph: &Graph) -> Result<(), StoreError> {
        let mut durable = self.durable();
        let mut db = self.open()?;
        let (snapshot, saved) = if graph.version() == Version::default() {
            (graph.snapshot()?, Version::default())
        } else {
            let now = Timestamp::now();
            let id = key_in(&mut db)?.public().to_string();
            graph.record_stored_at(&id, &durable, now)?;
            // The version before the snapshot is taken: the snapshot holds
            // at least that, and holds the record just made.
            let saved = graph.version();
            let snapshot = match graph.compaction_point_as(now, ACTIVE, &id, &saved) {
                Some(at) => graph.shallow_snapshot(&at)?,
                None => graph.snapshot()?,
            };
            (snapshot, saved)
        };
        debug!(bytes = snapshot.len(), "saving the snapshot");
        let tx = db.transaction()?;
        tx.execute(
            "INSERT INTO snapshot (id, data) SELECT ?1, data FROM snapshot WHERE id = ?2
             ON CONFLICT (id) DO UPDATE SET data = excluded.data",
            params![PREVIOUS, CURRENT],
        )?;
        tx.execute(
            "INSERT INTO snapshot (id, data) VALUES (?1, ?2)
             ON CONFLICT (id) DO UPDATE SET data = excluded.data",
            params![CURRENT, snapshot],
        )?;
        tx.commit()?;
        *durable = saved;
        Ok(())
    }

    /// The device's endpoint key, created on first use. Its public half is
    /// the device's ID.
    #[instrument(level = "debug", skip_all, fields(dir = %self.dir.display()))]
    pub fn key(&self) -> Result<SecretKey, StoreError> {
        self.ensure()?;
        key_in(&mut self.open()?)
    }

    /// Moves the graph's file out of the way, without deleting it, so a new
    /// graph can be made where it was. Returns where it went (nothing, if
    /// there was none). For a graph that can't be loaded, when someone has
    /// chosen to start again: the bytes stay, for anyone who can still make
    /// use of them.
    ///
    /// The device's identity isn't part of the graph that is set aside, so its
    /// key is carried into the new file if the old one will give it up. A file
    /// too damaged to read has no key to give, and the device gets a new one.
    #[instrument(level = "debug", skip_all, fields(dir = %self.dir.display()))]
    pub fn set_aside(&self) -> io::Result<Vec<PathBuf>> {
        let mut durable = self.durable();
        let mut moved = Vec::new();
        if self.file().exists() {
            let key = self.salvage_key();
            let stamp = Timestamp::now().as_millisecond();
            // SQLite pairs a journal with its file by name, so one left beside
            // the new file would be rolled back into it: they go together.
            for suffix in ["", "-journal", "-wal", "-shm"] {
                let from = self.dir.join(format!("{FILE}{suffix}"));
                let to = self.dir.join(format!("{FILE}.damaged-{stamp}{suffix}"));
                match fs::rename(&from, &to) {
                    Ok(()) if suffix.is_empty() => moved.push(to),
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
            }
            sync_dir(&self.dir)?;
            if let Some(key) = key {
                self.ensure().map_err(io::Error::other)?;
                self.open()
                    .map_err(io::Error::other)?
                    .execute(
                        "INSERT OR IGNORE INTO endpoint_key (id, secret) VALUES (1, ?1)",
                        params![key.as_slice()],
                    )
                    .map_err(io::Error::other)?;
            }
        }
        *durable = Version::default();
        Ok(moved)
    }

    /// The key in a file that may be damaged, if it will give it up. The file
    /// is opened read-only, which can't change it: a writer that finds a
    /// journal beside it would roll the journal back and delete it, and what
    /// is set aside is meant to be as it was found.
    fn salvage_key(&self) -> Option<[u8; 32]> {
        let db = Connection::open_with_flags(
            self.file(),
            OpenFlags::SQLITE_OPEN_READ_ONLY.union(OpenFlags::SQLITE_OPEN_NO_MUTEX),
        )
        .ok()?;
        db.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
            .ok()?;
        db.set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)
            .ok()?;
        db.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
            .ok()?;
        db.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_VIEW, false)
            .ok()?;
        let bytes: Vec<u8> = db
            .query_row("SELECT secret FROM endpoint_key WHERE id = 1", [], |r| {
                r.get(0)
            })
            .ok()?;
        bytes.try_into().ok()
    }

    /// Makes the file, with its schema and nothing in it, if there is none.
    /// It is built under another name and linked into place, so that it is
    /// never seen half made, and so that two making it at once agree: the
    /// first to link wins, and the other's is dropped. A directory of the old
    /// layout is converted instead.
    fn ensure(&self) -> Result<(), StoreError> {
        if self.file().exists() {
            return Ok(());
        }
        if self.legacy_present() {
            return self.migrate();
        }
        fs::create_dir_all(&self.dir)?;
        let staged = self.build()?;
        self.link(&staged)
    }

    /// Whether a directory of the old layout is here, to be converted.
    fn legacy_present(&self) -> bool {
        self.dir.join(LEGACY_SNAPSHOT).exists() || self.dir.join(LEGACY_KEY).exists()
    }

    /// Makes a file with the schema, under a name of its own. It is made
    /// before SQLite sees it so that it is never readable by anyone else:
    /// SQLite creates its journal with the file's own mode.
    fn build(&self) -> Result<PathBuf, StoreError> {
        let n = STAGING.fetch_add(1, Ordering::Relaxed);
        let staged = self
            .dir
            .join(format!("{FILE}.{}.{n}.new", std::process::id()));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        options.open(&staged)?;
        let made = (|| {
            let db = Self::connect_at(&staged)?;
            let mut statements = format!(
                "PRAGMA application_id = {APPLICATION_ID}; PRAGMA user_version = {LAYOUT}; BEGIN;"
            );
            for (_, sql) in TABLES {
                statements.push_str(sql);
                statements.push(';');
            }
            statements.push_str("COMMIT;");
            db.execute_batch(&statements)?;
            Ok::<(), StoreError>(())
        })();
        if let Err(e) = made {
            let _ = fs::remove_file(&staged);
            return Err(e);
        }
        Ok(staged)
    }

    /// Puts a built file in the graph's place, unless one is there already.
    fn link(&self, staged: &Path) -> Result<(), StoreError> {
        let linked = fs::hard_link(staged, self.file());
        let _ = fs::remove_file(staged);
        match linked {
            Ok(()) => Ok(sync_dir(&self.dir)?),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// A connection to the checked file, for what else is kept in it.
    /// The file is made, with no graph in it yet, if there is none: what
    /// keeps blobs may be opened before the graph is.
    pub(crate) fn connection(&self) -> Result<Connection, StoreError> {
        self.ensure()?;
        self.open()
    }

    /// Opens the file, checking that it is ours. A directory laid out as it
    /// was before the graph was one file is converted first.
    fn open(&self) -> Result<Connection, StoreError> {
        if !self.file().exists() {
            if !self.legacy_present() {
                return Err(StoreError::Missing(self.dir.clone()));
            }
            self.migrate()?;
        }
        let db = self.connect()?;
        let id: i32 = db.pragma_query_value(None, "application_id", |r| r.get(0))?;
        let layout: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if id != APPLICATION_ID {
            return Err(self.not_ours("it is not a Quiddity graph".into()));
        }
        if layout == 1 {
            // Layout 1 had no blobs; the table is the only difference.
            db.execute_batch(&format!(
                "BEGIN; {}; PRAGMA user_version = {LAYOUT}; COMMIT;",
                TABLES[0].1
            ))?;
            return self.open();
        }
        if layout != LAYOUT {
            return Err(self.not_ours(format!(
                "its layout is version {layout}, and this build reads version {LAYOUT}"
            )));
        }
        // Each statement is stored as written, and names its own kind and name,
        // so the statements alone say whether this is exactly our schema. Those
        // SQLite makes itself, for a `UNIQUE`, have none and are left out.
        let found: Vec<String> = db
            .prepare("SELECT sql FROM sqlite_schema WHERE sql IS NOT NULL ORDER BY name")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let mut expected: Vec<_> = TABLES.iter().collect();
        expected.sort();
        let matches = found.iter().eq(expected.iter().map(|(_, sql)| sql));
        if !matches {
            return Err(self.not_ours(
                "it holds more than a graph does (tables, views, triggers or indexes of another's)"
                    .into(),
            ));
        }
        Ok(db)
    }

    /// A connection with the file's protections on, whatever is in it.
    fn connect(&self) -> Result<Connection, StoreError> {
        Self::connect_at(&self.file())
    }

    fn connect_at(path: &Path) -> Result<Connection, StoreError> {
        // No `CREATE` (the file exists), no URI (a path is a path) and no
        // shared cache.
        let db = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE.union(OpenFlags::SQLITE_OPEN_NO_MUTEX),
        )?;
        db.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
        db.set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)?;
        db.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)?;
        db.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_VIEW, false)?;
        db.busy_timeout(BUSY)?;
        // One file at rest: a rollback journal, deleted at each commit, and
        // not write-ahead logging's `-wal` and `-shm` beside it. Overwriting
        // what is deleted matters for a file with a key in it.
        db.pragma_update(None, "journal_mode", "DELETE")?;
        db.pragma_update(None, "synchronous", "FULL")?;
        db.pragma_update(None, "secure_delete", "ON")?;
        Ok(db)
    }

    fn not_ours(&self, why: String) -> StoreError {
        StoreError::Unreadable(self.file(), why)
    }

    /// Makes the file from a directory of the old layout, then sets those
    /// files aside rather than deleting them, so that nothing is lost if this
    /// build is the one that is wrong.
    fn migrate(&self) -> Result<(), StoreError> {
        info!(dir = %self.dir.display(), "converting the graph to a single file");
        let read = |name: &str| match fs::read(self.dir.join(name)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        };
        // The snapshot is absent from a directory that only ever had its key
        // made, by asking for the device's ID before the graph was.
        let snapshot = read(LEGACY_SNAPSHOT)?;
        let previous = read(LEGACY_BACKUP)?;
        let key = match read(LEGACY_KEY)? {
            Some(bytes) => Some(
                <[u8; 32]>::try_from(bytes)
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad device key"))?,
            ),
            None => None,
        };
        // Built under another name and linked into place, so that a crash
        // leaves no graph.quiddity that is half made.
        let staged = self.build()?;
        let filled = (|| {
            let mut db = Self::connect_at(&staged)?;
            let tx = db.transaction()?;
            for (id, bytes) in [(CURRENT, &snapshot), (PREVIOUS, &previous)] {
                if let Some(bytes) = bytes {
                    tx.execute(
                        "INSERT INTO snapshot (id, data) VALUES (?1, ?2)",
                        params![id, bytes],
                    )?;
                }
            }
            if let Some(key) = key {
                tx.execute(
                    "INSERT INTO endpoint_key (id, secret) VALUES (1, ?1)",
                    params![key.as_slice()],
                )?;
            }
            tx.commit()?;
            Ok::<(), StoreError>(())
        })();
        if let Err(e) = filled {
            let _ = fs::remove_file(&staged);
            return Err(e);
        }
        self.link(&staged)?;
        for name in [LEGACY_SNAPSHOT, LEGACY_BACKUP, LEGACY_KEY] {
            let from = self.dir.join(name);
            if from.exists() {
                fs::rename(&from, self.dir.join(format!("{name}.{SET_ASIDE}")))?;
            }
        }
        Ok(())
    }
}

/// The key in the file, made if there is none. It is read without taking a
/// write lock, since it is asked for far more often than it is made; only
/// making it does, as `INSERT OR IGNORE` then `SELECT` in one transaction, so
/// that two processes starting at once agree.
fn key_in(db: &mut Connection) -> Result<SecretKey, StoreError> {
    let read = |db: &Connection| -> rusqlite::Result<Option<Vec<u8>>> {
        db.query_row("SELECT secret FROM endpoint_key WHERE id = 1", [], |r| {
            r.get(0)
        })
        .optional()
    };
    let bytes = match read(db)? {
        Some(bytes) => bytes,
        None => {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let made = SecretKey::generate();
            info!("making this device's endpoint key");
            tx.execute(
                "INSERT OR IGNORE INTO endpoint_key (id, secret) VALUES (1, ?1)",
                params![made.to_bytes().as_slice()],
            )?;
            let bytes = read(&tx)?.expect("the key was just inserted, or was there");
            tx.commit()?;
            bytes
        }
    };
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad endpoint key"))?;
    Ok(SecretKey::from_bytes(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A device on its own needs no history: once it has saved, what it
    /// stores is compacted, and records that it stored it.
    #[test]
    fn saving_records_the_device_and_compacts() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        let graph = store.init(false).unwrap();
        let id = store.key().unwrap().public().to_string();
        let block = graph.create_block(None, None, "").unwrap();
        for i in 0..300 {
            graph
                .set_text(block, &format!("draft {i} ").repeat(10))
                .unwrap();
            graph.commit();
        }
        store.save(&graph).unwrap();

        let saved: Vec<u8> = store
            .open()
            .unwrap()
            .query_row("SELECT data FROM snapshot", [], |r| r.get(0))
            .unwrap();
        assert!(saved.len() * 4 < graph.snapshot().unwrap().len());
        let loaded = store.load().unwrap();
        assert_eq!(loaded.text(block).unwrap(), graph.text(block).unwrap());
        assert_ne!(loaded.history_start(), Version::default());
        let devices = loaded.devices();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, id);
    }

    /// Another device holds compaction back for 30 days after it was last
    /// heard of, then stops.
    #[test]
    fn silent_devices_hold_compaction_back_for_a_month() {
        for (days, compacted) in [(29, false), (31, true)] {
            let tmp = tempfile::tempdir().unwrap();
            let store = Store::new(tmp.path());
            let graph = store.init(false).unwrap();
            let seen = Timestamp::now() - SignedDuration::from_hours(24 * days);
            graph
                .vouch_for("silent", &Version::default(), seen)
                .unwrap();
            graph.create_block(None, None, "since").unwrap();
            graph.commit();
            store.save(&graph).unwrap();
            let start = store.load().unwrap().history_start();
            assert_eq!(start != Version::default(), compacted, "after {days} days");
        }
    }

    /// What a device says it has stored is only ever what is already on disk,
    /// so a save that fails can't leave the graph (and so other devices)
    /// believing in a version this device doesn't have.
    #[test]
    fn a_failed_save_claims_nothing_new() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        let graph = store.init(false).unwrap();
        let id = store.key().unwrap().public().to_string();
        let claim = |graph: &Graph| {
            graph
                .devices()
                .into_iter()
                .find(|d| d.id == id)
                .unwrap()
                .stored
        };
        let on_disk = store.load().unwrap().version();

        // Make the commit fail: another connection is in the middle of reading,
        // which a rollback journal can't commit past.
        let reader = Connection::open(store.file()).unwrap();
        reader
            .execute_batch("BEGIN; SELECT * FROM snapshot;")
            .unwrap();
        graph.create_block(None, None, "unsaved").unwrap();
        graph.commit();
        let edited = graph.version();
        store.save(&graph).unwrap_err();

        assert!(!claim(&graph).includes(&edited));
        assert_eq!(claim(&graph), on_disk);
        // And once the reader is done, the save goes through.
        drop(reader);
        store.save(&graph).unwrap();
        assert_eq!(store.load().unwrap().version(), graph.version());
    }

    /// A save records what the snapshot held before it, whether that was
    /// written by the previous save or found by `load`.
    #[test]
    fn a_save_records_what_the_file_held_before() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        let graph = store.init(false).unwrap();
        let id = store.key().unwrap().public().to_string();
        let claim = |graph: &Graph| {
            graph
                .devices()
                .into_iter()
                .find(|d| d.id == id)
                .unwrap()
                .stored
        };
        let first = store.load().unwrap().version();
        graph.create_block(None, None, "second").unwrap();
        graph.commit();
        store.save(&graph).unwrap();
        assert_eq!(claim(&graph), first);

        // A fresh handle that loads the file records what it found there.
        let loaded = Store::new(tmp.path());
        let graph = loaded.load().unwrap();
        let found = graph.version();
        graph.create_block(None, None, "third").unwrap();
        graph.commit();
        loaded.save(&graph).unwrap();
        assert_eq!(claim(&graph), found);
    }

    /// Each save leaves the one before it as the previous generation, and a
    /// snapshot that can't be decoded falls back to it.
    #[test]
    fn a_save_keeps_the_previous_snapshot() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        let graph = store.init(false).unwrap();
        let block = graph.create_block(None, None, "first").unwrap();
        graph.commit();
        store.save(&graph).unwrap();
        graph.set_text(block, "second").unwrap();
        graph.commit();
        store.save(&graph).unwrap();

        store
            .connect()
            .unwrap()
            .execute("UPDATE snapshot SET data = x'' WHERE id = 1", [])
            .unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.text(block).unwrap(), "first");
    }

    /// Without a previous generation a bad snapshot is an error, not an
    /// empty graph.
    #[test]
    fn a_bad_snapshot_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        store.init(false).unwrap();
        store
            .connect()
            .unwrap()
            .execute("UPDATE snapshot SET data = x'' WHERE id = 1", [])
            .unwrap();
        assert!(store.load().is_err());
    }

    /// Savers on separate threads don't trample each other.
    #[test]
    fn concurrent_saves_all_succeed() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        let graph = store.init(false).unwrap();
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    for _ in 0..10 {
                        store.save(&graph).unwrap();
                    }
                });
            }
        });
        store.load().unwrap();
    }

    /// The old layout's backup comes with it, as the previous generation.
    #[test]
    fn the_old_layouts_backup_is_the_previous_generation() {
        let tmp = tempfile::tempdir().unwrap();
        let graph = Graph::new();
        let block = graph.create_block(None, None, "before").unwrap();
        graph.commit();
        fs::write(tmp.path().join(LEGACY_BACKUP), graph.snapshot().unwrap()).unwrap();
        graph.set_text(block, "now").unwrap();
        graph.commit();
        fs::write(tmp.path().join(LEGACY_SNAPSHOT), graph.snapshot().unwrap()).unwrap();

        let store = Store::new(tmp.path());
        assert_eq!(store.load().unwrap().text(block).unwrap(), "now");
        store
            .connect()
            .unwrap()
            .execute("UPDATE snapshot SET data = x'' WHERE id = 1", [])
            .unwrap();
        assert_eq!(store.load().unwrap().text(block).unwrap(), "before");
        assert!(!tmp.path().join(LEGACY_BACKUP).exists());
    }

    /// A blank replica saves as it is, and doesn't record itself: that
    /// would be an edit made before it has joined.
    #[test]
    fn a_blank_replica_records_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        store.init(true).unwrap();
        assert_eq!(store.load().unwrap().version(), Version::default());
    }

    /// A directory with no graph says so, and where, rather than reporting
    /// a missing file.
    #[test]
    fn loading_where_there_is_no_graph_names_the_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let err = Store::new(tmp.path()).load().unwrap_err();
        assert!(matches!(err, crate::StoreError::Missing(_)), "{err:?}");
        let message = err.to_string();
        assert!(message.contains(&tmp.path().display().to_string()));
        assert!(message.contains("init"));
    }

    /// The key is made once and then kept, and a stored key of the wrong
    /// length is reported rather than replaced.
    #[test]
    fn the_endpoint_key_is_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        store.init(true).unwrap();
        let key = store.key().unwrap();
        assert_eq!(store.key().unwrap().public(), key.public());
        assert_eq!(Store::new(tmp.path()).key().unwrap().public(), key.public());
    }

    /// The file holds the secret key, so nobody else may read it, nor the
    /// journal SQLite keeps beside it while saving.
    #[cfg(unix)]
    #[test]
    fn the_file_is_private_to_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        store.init(false).unwrap();
        let mode = fs::metadata(store.file()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    /// Saving leaves one file and nothing else beside it.
    #[test]
    fn a_saved_graph_is_one_file() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        let graph = store.init(false).unwrap();
        graph.create_block(None, None, "text").unwrap();
        graph.commit();
        store.save(&graph).unwrap();
        let names: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, [FILE]);
    }

    /// What is not a graph of ours is refused, whatever else it is: another
    /// application's database, a layout from the future, and one with
    /// triggers or tables we didn't make.
    #[test]
    fn a_file_that_is_not_ours_is_refused() {
        let cases: [(&str, &str); 6] = [
            ("another application", "PRAGMA application_id = 7"),
            ("a later layout", "PRAGMA user_version = 3"),
            (
                "a trigger",
                "CREATE TRIGGER t AFTER UPDATE ON snapshot BEGIN SELECT 1; END",
            ),
            ("an extra table", "CREATE TABLE extra (x)"),
            // The same number of tables, with the right names, but not ours.
            (
                "a redefined table",
                "DROP TABLE endpoint_key; CREATE TABLE endpoint_key (id INTEGER PRIMARY KEY, secret BLOB)",
            ),
            (
                "a view where a table was",
                "DROP TABLE endpoint_key; CREATE VIEW endpoint_key AS SELECT 1 AS id, 2 AS secret",
            ),
        ];
        for (what, sql) in cases {
            let tmp = tempfile::tempdir().unwrap();
            let store = Store::new(tmp.path());
            store.init(false).unwrap();
            store.connect().unwrap().execute_batch(sql).unwrap();
            let err = store.load().unwrap_err();
            assert!(matches!(err, StoreError::Unreadable(..)), "{what}: {err:?}");
            assert!(store.save(&Graph::new()).is_err(), "{what}");
        }
    }

    /// A file that is not SQLite at all says so too.
    #[test]
    fn a_file_that_is_not_sqlite_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join(FILE),
            b"not a database, but long enough to look like one",
        )
        .unwrap();
        assert!(Store::new(tmp.path()).load().is_err());
    }

    /// A directory from before the graph was one file is converted: its
    /// snapshot and its key come with it, and the old files are set aside.
    #[test]
    fn the_old_layout_is_converted() {
        let tmp = tempfile::tempdir().unwrap();
        let graph = Graph::new();
        let block = graph.create_block(None, None, "kept").unwrap();
        graph.commit();
        let key = SecretKey::generate();
        fs::write(tmp.path().join(LEGACY_SNAPSHOT), graph.snapshot().unwrap()).unwrap();
        fs::write(tmp.path().join(LEGACY_KEY), key.to_bytes()).unwrap();

        let store = Store::new(tmp.path());
        assert_eq!(store.load().unwrap().text(block).unwrap(), "kept");
        assert_eq!(store.key().unwrap().public(), key.public());
        assert!(tmp.path().join(FILE).exists());
        assert!(!tmp.path().join(LEGACY_SNAPSHOT).exists());
        assert!(
            tmp.path()
                .join(format!("{LEGACY_SNAPSHOT}.{SET_ASIDE}"))
                .exists()
        );
        // And it is not done twice.
        assert_eq!(store.load().unwrap().text(block).unwrap(), "kept");
    }

    /// A graph that was never given a key (a snapshot only) is converted,
    /// and gets one when asked.
    #[test]
    fn the_old_layout_without_a_key_is_converted() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join(LEGACY_SNAPSHOT),
            Graph::new().snapshot().unwrap(),
        )
        .unwrap();
        let store = Store::new(tmp.path());
        store.load().unwrap();
        assert_eq!(store.key().unwrap().public(), store.key().unwrap().public());
    }

    /// Setting a graph aside keeps every byte, and leaves nothing to load,
    /// and the device keeps its identity.
    #[test]
    fn setting_aside_keeps_the_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        assert!(store.set_aside().unwrap().is_empty());

        let graph = store.init(false).unwrap();
        store.save(&graph).unwrap();
        let key = store.key().unwrap();
        let before = fs::read(store.file()).unwrap();
        let moved = store.set_aside().unwrap();

        assert_eq!(moved.len(), 1);
        assert_eq!(fs::read(&moved[0]).unwrap(), before);
        assert!(matches!(
            store.load().unwrap_err(),
            crate::StoreError::Missing(_)
        ));
        // The device's identity isn't part of the graph that was set aside.
        assert_eq!(store.key().unwrap().public(), key.public());

        // What the old file held says nothing about the new one: a graph
        // started now has stored nothing yet.
        let fresh = store.init(false).unwrap();
        let id = store.key().unwrap().public().to_string();
        let own = fresh.devices().into_iter().find(|d| d.id == id).unwrap();
        assert_eq!(own.stored, Version::default());
    }

    /// A file too damaged to read is set aside as it is, with its journal (or
    /// the next file of that name would have it rolled back into it), and the
    /// device starts again with a graph and an identity of its own.
    #[test]
    fn a_damaged_file_is_set_aside_whole() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        fs::write(
            store.file(),
            b"not a database, and long enough to look like one",
        )
        .unwrap();
        assert!(store.load().is_err());
        assert!(matches!(store.init(false), Err(StoreError::Exists(_))));
        // Opening the file for real is SQLite's to do as it likes with a
        // journal, so the journal is put there after.
        fs::write(tmp.path().join(format!("{FILE}-journal")), b"a hot journal").unwrap();

        let moved = store.set_aside().unwrap();
        assert_eq!(moved.len(), 1);
        assert_eq!(
            fs::read(&moved[0]).unwrap(),
            b"not a database, and long enough to look like one"
        );
        let journal = format!("{}-journal", moved[0].display());
        assert_eq!(fs::read(journal).unwrap(), b"a hot journal");
        assert!(!tmp.path().join(format!("{FILE}-journal")).exists());

        store.init(false).unwrap();
        store.load().unwrap();
    }

    /// The device's ID can be asked for before there is a graph, and is the
    /// same one afterwards.
    #[test]
    fn a_key_asked_for_first_is_kept_by_init() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        let key = store.key().unwrap();
        assert!(matches!(
            store.load().unwrap_err(),
            crate::StoreError::Missing(_)
        ));
        store.init(false).unwrap();
        assert_eq!(store.key().unwrap().public(), key.public());
    }

    /// Two processes asking for the key of a new device at once agree on it.
    #[test]
    fn simultaneous_first_uses_agree_on_the_key() {
        for _ in 0..8 {
            let tmp = tempfile::tempdir().unwrap();
            let keys: Vec<_> = std::thread::scope(|s| {
                let handles: Vec<_> = (0..4)
                    .map(|_| s.spawn(|| Store::new(tmp.path()).key().unwrap().to_bytes()))
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });
            assert!(keys.windows(2).all(|pair| pair[0] == pair[1]));
        }
    }

    /// A directory of the old layout that only ever had its key, made by
    /// asking for the device's ID, keeps that ID.
    #[test]
    fn an_old_directory_with_only_a_key_keeps_it() {
        let tmp = tempfile::tempdir().unwrap();
        let key = SecretKey::generate();
        fs::write(tmp.path().join(LEGACY_KEY), key.to_bytes()).unwrap();
        let store = Store::new(tmp.path());
        assert_eq!(store.key().unwrap().public(), key.public());
        assert!(matches!(
            store.load().unwrap_err(),
            crate::StoreError::Missing(_)
        ));
    }

    /// A file of the first layout, which had no blobs, is upgraded in place:
    /// what it held is kept and it gains the table.
    #[test]
    fn layout_one_gains_the_blob_table() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        let graph = store.init(false).unwrap();
        let block = graph.create_block(None, None, "kept").unwrap();
        graph.commit();
        store.save(&graph).unwrap();
        store
            .connect()
            .unwrap()
            .execute_batch("DROP TABLE blob; PRAGMA user_version = 1;")
            .unwrap();

        assert_eq!(store.load().unwrap().text(block).unwrap(), "kept");
        let db = store.connection().unwrap();
        let layout: i32 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(layout, LAYOUT);
        db.execute(
            "INSERT INTO blob (hash, size, outboard, data) VALUES (zeroblob(32), 1, x'', x'00')",
            [],
        )
        .unwrap();
    }

    /// Making a graph where one is already refuses, in either layout.
    #[test]
    fn init_does_not_replace_a_graph() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        store.init(false).unwrap();
        assert!(matches!(store.init(false), Err(StoreError::Exists(_))));
        let old = tempfile::tempdir().unwrap();
        fs::write(old.path().join(LEGACY_SNAPSHOT), b"x").unwrap();
        assert!(matches!(
            Store::new(old.path()).init(false),
            Err(StoreError::Exists(_))
        ));
    }
}
