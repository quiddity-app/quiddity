//! A graph on disk: one SQLite file holding the graph's snapshot and the
//! device's endpoint key.
//!
//! The device's own key is not in it (see [`Keyring`]), so copying the file
//! doesn't copy the device. A new graph's endpoint key is derived from that
//! key and a random salt kept in the file. A graph made before then holds its
//! endpoint key itself, which is kept so that its device ID is the same.
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
//! schema-defined functions off. An old graph's file holds its secret key, so
//! it is created readable by its owner only.

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
use crate::keyring::Keyring;
use quiddity_core::NodeId;
use quiddity_identity::{Salt, SealedNodeId};

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
///
/// 1: the snapshot, and the device's endpoint key. 2: and the salt that a new
/// graph's endpoint key is derived with. 3: and this replica's node ID.
const LAYOUT: i32 = 3;

/// Exactly the schema, as SQLite stores it (its `sql` column is the statement
/// as written), so that a file with anything else in it is refused. One row
/// each, enforced by the key, so there is no question which is current.
/// The snapshot has two: row 1 is the current one, and row 2 the one before
/// it, which `load` falls back to if row 1 can't be decoded.
const TABLES: [(&str, &str); 4] = [SNAPSHOT_TABLE, ENDPOINT_KEY_TABLE, SLOT_TABLE, NODE_TABLE];

const SNAPSHOT_TABLE: (&str, &str) = (
    "snapshot",
    "CREATE TABLE snapshot (id INTEGER PRIMARY KEY CHECK (id IN (1, 2)), data BLOB NOT NULL) STRICT",
);
/// Only a graph made before the device's key was kept apart has a row here,
/// and keeps it so that its device ID doesn't change.
const ENDPOINT_KEY_TABLE: (&str, &str) = (
    "endpoint_key",
    "CREATE TABLE endpoint_key (id INTEGER PRIMARY KEY CHECK (id = 1), secret BLOB NOT NULL CHECK (length(secret) = 32)) STRICT",
);
/// One row: random, and not secret. What a derived key is specific to.
const SLOT_TABLE: (&str, &str) = (
    "slot",
    "CREATE TABLE slot (id INTEGER PRIMARY KEY CHECK (id = 1), salt BLOB NOT NULL CHECK (length(salt) = 16)) STRICT",
);

/// The replica's node ID, sealed (see [`SealedNodeId`]): readable only on the
/// device that made it, so that a file taken to another is known to have been.
const NODE_TABLE: (&str, &str) = (
    "node",
    "CREATE TABLE node (id INTEGER PRIMARY KEY CHECK (id = 1), sealed BLOB NOT NULL CHECK (length(sealed) = 12)) STRICT",
);

/// What earlier layouts held, which are converted when they are opened.
const TABLES_V1: [(&str, &str); 2] = [SNAPSHOT_TABLE, ENDPOINT_KEY_TABLE];
const TABLES_V2: [(&str, &str); 3] = [SNAPSHOT_TABLE, ENDPOINT_KEY_TABLE, SLOT_TABLE];

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
    /// Where this device's key is, for a graph that derives its endpoint key.
    keyring: Keyring,
    /// Serialises saves, which come from the command thread and the
    /// save-on-change task at once, and holds the version the snapshot in the
    /// file is known to hold: what a save may say this device has stored.
    /// Clones share it.
    durable: Arc<Mutex<Version>>,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>, keyring: Keyring) -> Self {
        Self {
            dir: dir.into(),
            keyring,
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
        let node = self.node_id()?;
        let graph = if blank {
            let graph = Graph::blank();
            graph.set_node_id(node)?;
            graph
        } else {
            Graph::new_as(node)
        };
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
        // The replica that is kept edits as the node it has always been, so that
        // opening a graph a hundred times doesn't add a hundred peers to it.
        let node = self.node_id()?;
        match Graph::from_snapshot(&bytes) {
            Ok(graph) => {
                graph.set_node_id(node)?;
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
                graph.set_node_id(node)?;
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
            let id = self.key_in(&db)?.public().to_string();
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

    /// The device's endpoint key for this graph, made on first use. Its public
    /// half is the device's ID.
    ///
    /// A graph made before the device's key was kept apart has its own, in its
    /// file. Any other derives it from the device's key and the file's salt, so
    /// each graph has an ID of its own, and a copy of the file taken to another
    /// device has another.
    #[instrument(level = "debug", skip_all, fields(dir = %self.dir.display()))]
    pub fn key(&self) -> Result<SecretKey, StoreError> {
        self.ensure()?;
        self.key_in(&self.open()?)
    }

    /// The node ID this replica edits as, made on first use and kept.
    ///
    /// It is stored sealed with a mask derived from the device's key and the
    /// file's salt, so it only opens on the device that made it. A file that
    /// was made on another device, or that has lost its device's key, has one
    /// that doesn't open: the replica is not that one, and gets a node ID of its
    /// own. Edits made under the old ID stay as they are, and are the other
    /// device's.
    #[instrument(level = "debug", skip_all, fields(dir = %self.dir.display()))]
    pub fn node_id(&self) -> Result<NodeId, StoreError> {
        self.ensure()?;
        self.node_id_in(&mut self.open()?)
    }

    fn node_id_in(&self, db: &mut Connection) -> Result<NodeId, StoreError> {
        let mask = self.keyring.key()?.node_mask(&salt_in(db)?);
        let read = |db: &Connection| -> Result<Option<SealedNodeId>, StoreError> {
            let sealed: Option<Vec<u8>> = db
                .query_row("SELECT sealed FROM node WHERE id = 1", [], |r| r.get(0))
                .optional()?;
            Ok(sealed.and_then(|bytes| Some(SealedNodeId::from_bytes(bytes.try_into().ok()?))))
        };
        if let Some(id) = read(db)?.and_then(|sealed| sealed.open(&mask)) {
            return Ok(id);
        }
        // None, or not made here. Made under a write lock, looking again first,
        // so that two processes asking together agree on one.
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let stored = read(&tx)?;
        if let Some(id) = stored.and_then(|sealed| sealed.open(&mask)) {
            return Ok(id);
        }
        let id = Graph::fresh_node_id();
        tx.execute(
            "INSERT INTO node (id, sealed) VALUES (1, ?1)
             ON CONFLICT (id) DO UPDATE SET sealed = excluded.sealed",
            params![id.seal(&mask).to_bytes().as_slice()],
        )?;
        tx.commit()?;
        if stored.is_some() {
            warn!("this graph's node ID was made on another device: this replica has a new one");
        } else {
            info!("making this replica's node ID");
        }
        Ok(id)
    }

    fn key_in(&self, db: &Connection) -> Result<SecretKey, StoreError> {
        let own: Option<Vec<u8>> = db
            .query_row("SELECT secret FROM endpoint_key WHERE id = 1", [], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(bytes) = own {
            let bytes: [u8; 32] = bytes
                .try_into()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad endpoint key"))?;
            return Ok(SecretKey::from_bytes(&bytes));
        }
        let salt = salt_in(db).map_err(|e| match e {
            StoreError::Sqlite(_) => self.not_ours("it has no salt".into()),
            e => e,
        })?;
        let seed = self.keyring.key()?.endpoint_seed(&salt);
        Ok(SecretKey::from_bytes(&seed))
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
            let kept = self.salvage();
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
            if kept.key.is_some() || kept.salt.is_some() {
                self.ensure().map_err(io::Error::other)?;
                let db = self.open().map_err(io::Error::other)?;
                if let Some(key) = kept.key {
                    db.execute(
                        "INSERT OR IGNORE INTO endpoint_key (id, secret) VALUES (1, ?1)",
                        params![key.as_slice()],
                    )
                    .map_err(io::Error::other)?;
                }
                if let Some(salt) = kept.salt {
                    db.execute(
                        "UPDATE slot SET salt = ?1 WHERE id = 1",
                        params![salt.as_slice()],
                    )
                    .map_err(io::Error::other)?;
                }
            }
        }
        *durable = Version::default();
        Ok(moved)
    }

    /// What gives the device its identity, in a file that may be damaged, if it
    /// will give it up: the endpoint key of a graph that has its own, and the
    /// salt. The file is opened read-only, which can't change it: a writer that
    /// finds a journal beside it would roll the journal back and delete it, and
    /// what is set aside is meant to be as it was found.
    fn salvage(&self) -> Salvaged {
        (|| {
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
            let blob = |sql: &str| -> Option<Vec<u8>> { db.query_row(sql, [], |r| r.get(0)).ok() };
            Some(Salvaged {
                key: blob("SELECT secret FROM endpoint_key WHERE id = 1")
                    .and_then(|b| b.try_into().ok()),
                salt: blob("SELECT salt FROM slot WHERE id = 1").and_then(|b| b.try_into().ok()),
            })
        })()
        .unwrap_or_default()
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
            db.execute(
                "INSERT INTO slot (id, salt) VALUES (1, ?1)",
                params![new_salt()?.as_slice()],
            )?;
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

    /// Opens the file, checking that it is ours. A directory laid out as it
    /// was before the graph was one file is converted first.
    fn open(&self) -> Result<Connection, StoreError> {
        if !self.file().exists() {
            if !self.legacy_present() {
                return Err(StoreError::Missing(self.dir.clone()));
            }
            self.migrate()?;
        }
        let mut db = self.connect()?;
        let id: i32 = db.pragma_query_value(None, "application_id", |r| r.get(0))?;
        let layout: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if id != APPLICATION_ID {
            return Err(self.not_ours("it is not a Quiddity graph".into()));
        }
        match layout {
            LAYOUT => {}
            1 | 2 => {
                self.check_schema(&db, if layout == 1 { &TABLES_V1 } else { &TABLES_V2 })?;
                self.upgrade(&mut db)?;
            }
            _ => {
                return Err(self.not_ours(format!(
                    "its layout is version {layout}, and this build reads version {LAYOUT}"
                )));
            }
        }
        self.check_schema(&db, &TABLES)?;
        Ok(db)
    }

    /// Whether the file's schema is exactly `tables`. Each statement is stored
    /// as written, and names its own kind and name, so the statements alone
    /// say whether this is exactly our schema. Those SQLite makes itself, for
    /// a `UNIQUE`, have none and are left out.
    fn check_schema(&self, db: &Connection, tables: &[(&str, &str)]) -> Result<(), StoreError> {
        let found: Vec<String> = db
            .prepare("SELECT sql FROM sqlite_schema WHERE sql IS NOT NULL ORDER BY name")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let mut expected: Vec<_> = tables.iter().collect();
        expected.sort();
        if !found.iter().eq(expected.iter().map(|(_, sql)| sql)) {
            return Err(self.not_ours(
                "it holds more than a graph does (tables, views, triggers or indexes of another's)"
                    .into(),
            ));
        }
        Ok(())
    }

    /// Converts a file of an earlier layout: layout 1 is given a salt, which
    /// only a graph that derives its key uses (it keeps its own endpoint key),
    /// and any earlier than 3 is given a place for its node ID, made when it
    /// is first wanted. Under a write lock, and looking again once it has it,
    /// so that two processes opening the file together convert it once.
    fn upgrade(&self, db: &mut Connection) -> Result<(), StoreError> {
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let layout: i32 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if layout < LAYOUT {
            info!(layout, "converting the graph's file to layout {LAYOUT}");
            if layout == 1 {
                tx.execute_batch(SLOT_TABLE.1)?;
                tx.execute(
                    "INSERT INTO slot (id, salt) VALUES (1, ?1)",
                    params![new_salt()?.as_slice()],
                )?;
            }
            tx.execute_batch(NODE_TABLE.1)?;
            tx.pragma_update(None, "user_version", LAYOUT)?;
        }
        tx.commit()?;
        Ok(())
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

/// What [`Store::salvage`] found.
#[derive(Debug, Default)]
struct Salvaged {
    key: Option<[u8; 32]>,
    salt: Option<[u8; 16]>,
}

/// The salt in the file.
fn salt_in(db: &Connection) -> Result<Salt, StoreError> {
    let bytes: Vec<u8> = db.query_row("SELECT salt FROM slot WHERE id = 1", [], |r| r.get(0))?;
    let bytes: [u8; 16] = bytes
        .try_into()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad salt"))?;
    Ok(Salt::from_bytes(bytes))
}

/// A salt for a new file.
fn new_salt() -> Result<[u8; 16], StoreError> {
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(salt)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The store of the graph in `dir`, for a device whose key is in `dir/keys`
    /// (so that two stores of one directory are one device).
    fn store_at(dir: impl AsRef<Path>) -> Store {
        let dir = dir.as_ref();
        Store::new(dir, Keyring::new(dir.join("keys")))
    }

    /// Makes the file what an earlier build made: layout 1 had no salt, and
    /// layouts 1 and 2 had no node ID.
    fn as_layout(store: &Store, layout: i32) {
        let tables = match layout {
            1 => "DROP TABLE node; DROP TABLE slot;",
            2 => "DROP TABLE node;",
            _ => panic!("no such earlier layout"),
        };
        store
            .connect()
            .unwrap()
            .execute_batch(&format!("{tables} PRAGMA user_version = {layout};"))
            .unwrap();
    }

    fn layout(store: &Store) -> i32 {
        store
            .connect()
            .unwrap()
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap()
    }

    /// A device on its own needs no history: once it has saved, what it
    /// stores is compacted, and records that it stored it.
    #[test]
    fn saving_records_the_device_and_compacts() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_at(tmp.path());
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
            let store = store_at(tmp.path());
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
        let store = store_at(tmp.path());
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
        let store = store_at(tmp.path());
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
        let loaded = store_at(tmp.path());
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
        let store = store_at(tmp.path());
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
        let store = store_at(tmp.path());
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
        let store = store_at(tmp.path());
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

        let store = store_at(tmp.path());
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
        let store = store_at(tmp.path());
        store.init(true).unwrap();
        assert_eq!(store.load().unwrap().version(), Version::default());
    }

    /// A directory with no graph says so, and where, rather than reporting
    /// a missing file.
    #[test]
    fn loading_where_there_is_no_graph_names_the_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let err = store_at(tmp.path()).load().unwrap_err();
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
        let store = store_at(tmp.path());
        store.init(true).unwrap();
        let key = store.key().unwrap();
        assert_eq!(store.key().unwrap().public(), key.public());
        assert_eq!(store_at(tmp.path()).key().unwrap().public(), key.public());
    }

    /// The file holds the secret key, so nobody else may read it, nor the
    /// journal SQLite keeps beside it while saving.
    #[cfg(unix)]
    #[test]
    fn the_file_is_private_to_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let store = store_at(tmp.path());
        store.init(false).unwrap();
        let mode = fs::metadata(store.file()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    /// Saving leaves one file and nothing else beside it.
    #[test]
    fn a_saved_graph_is_one_file() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("graph");
        let store = Store::new(&dir, Keyring::new(tmp.path().join("keys")));
        let graph = store.init(false).unwrap();
        graph.create_block(None, None, "text").unwrap();
        graph.commit();
        store.save(&graph).unwrap();
        let names: Vec<_> = fs::read_dir(&dir)
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
            ("a later layout", "PRAGMA user_version = 4"),
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
            let store = store_at(tmp.path());
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
        assert!(store_at(tmp.path()).load().is_err());
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

        let store = store_at(tmp.path());
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
        let store = store_at(tmp.path());
        store.load().unwrap();
        assert_eq!(store.key().unwrap().public(), store.key().unwrap().public());
    }

    /// Setting a graph aside keeps every byte, and leaves nothing to load,
    /// and the device keeps its identity.
    #[test]
    fn setting_aside_keeps_the_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_at(tmp.path());
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
        let store = store_at(tmp.path());
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
        let store = store_at(tmp.path());
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
                    .map(|_| s.spawn(|| store_at(tmp.path()).key().unwrap().to_bytes()))
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
        let store = store_at(tmp.path());
        assert_eq!(store.key().unwrap().public(), key.public());
        assert!(matches!(
            store.load().unwrap_err(),
            crate::StoreError::Missing(_)
        ));
    }

    /// Making a graph where one is already refuses, in either layout.
    #[test]
    fn init_does_not_replace_a_graph() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_at(tmp.path());
        store.init(false).unwrap();
        assert!(matches!(store.init(false), Err(StoreError::Exists(_))));
        let old = tempfile::tempdir().unwrap();
        fs::write(old.path().join(LEGACY_SNAPSHOT), b"x").unwrap();
        assert!(matches!(
            store_at(old.path()).init(false),
            Err(StoreError::Exists(_))
        ));
    }

    /// What each way of getting a key gives: the derived key is exactly what
    /// the device's key and the file's salt say, and nothing of it is kept in
    /// the file.
    #[test]
    fn a_new_graphs_key_is_derived_from_the_device_key_and_the_files_salt() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_at(tmp.path());
        store.init(false).unwrap();
        let key = store.key().unwrap();

        let db = store.connect().unwrap();
        let held: i64 = db
            .query_row("SELECT count(*) FROM endpoint_key", [], |r| r.get(0))
            .unwrap();
        assert_eq!(held, 0, "no key of its own");
        let salt = salt_in(&db).unwrap();
        let device = store.keyring.key().unwrap();
        assert_eq!(
            key.to_bytes(),
            *device.endpoint_seed(&salt),
            "it is what the device key and the salt say"
        );
        assert_ne!(key.to_bytes(), salt.to_bytes().repeat(2)[..32]);

        // And neither it nor the device's key is in the file.
        store.save(&Graph::new()).unwrap();
        let file = fs::read(store.file()).unwrap();
        let contains = |needle: &[u8]| file.windows(needle.len()).any(|w| w == needle);
        assert!(
            !contains(&key.to_bytes()),
            "the endpoint key is in the file"
        );
        assert!(contains(&salt.to_bytes()), "the salt is");
        assert!(!contains(device.to_token().as_bytes()));
        assert_eq!(
            store.key().unwrap().to_bytes(),
            key.to_bytes(),
            "and is stable"
        );
    }

    /// Each graph a device holds is a device of its own, and a file taken to
    /// another device is another device: neither copies the one that made it.
    #[test]
    fn graphs_are_devices_of_their_own_and_a_copy_on_another_device_is_another() {
        let tmp = tempfile::tempdir().unwrap();
        let keys = Keyring::new(tmp.path().join("keys"));
        let at = |name: &str, keys: &Keyring| Store::new(tmp.path().join(name), keys.clone());

        let (one, two) = (at("one", &keys), at("two", &keys));
        one.init(false).unwrap();
        two.init(false).unwrap();
        assert_ne!(one.key().unwrap().public(), two.key().unwrap().public());

        // The file copied beside it, for the same device: the same device,
        // which is what the map of where graphs are exists to notice.
        let copy = at("copy", &keys);
        fs::create_dir_all(copy.dir()).unwrap();
        fs::copy(one.file(), copy.file()).unwrap();
        assert_eq!(copy.key().unwrap().public(), one.key().unwrap().public());

        // The file copied to a device with another key: another device.
        let elsewhere = at("elsewhere", &Keyring::new(tmp.path().join("other keys")));
        fs::create_dir_all(elsewhere.dir()).unwrap();
        fs::copy(one.file(), elsewhere.file()).unwrap();
        assert_ne!(
            elsewhere.key().unwrap().public(),
            one.key().unwrap().public()
        );
    }

    /// The device's key is shared by every graph, and made once.
    #[test]
    fn every_graph_on_a_device_uses_its_one_key() {
        let tmp = tempfile::tempdir().unwrap();
        let keys = Keyring::new(tmp.path().join("keys"));
        for name in ["a", "b", "c"] {
            let store = Store::new(tmp.path().join(name), keys.clone());
            store.init(false).unwrap();
            store.key().unwrap();
        }
        assert_eq!(fs::read_dir(keys.dir()).unwrap().count(), 1);
    }

    /// The file's own salts differ, or the graphs would be one device.
    #[test]
    fn each_file_has_a_salt_of_its_own() {
        let tmp = tempfile::tempdir().unwrap();
        let salts: Vec<_> = (0..8)
            .map(|i| {
                let store = store_at(tmp.path().join(format!("g{i}")));
                store.init(false).unwrap();
                salt_in(&store.connect().unwrap()).unwrap().to_bytes()
            })
            .collect();
        let distinct: std::collections::HashSet<_> = salts.iter().collect();
        assert_eq!(distinct.len(), salts.len());
        assert!(salts.iter().all(|s| s != &[0; 16]));
    }

    /// A graph made before the device's key was kept apart (layout 1) keeps its
    /// own key, and so its device ID, and doesn't ask for the device's to say
    /// what it is: it is only given a salt.
    #[test]
    fn a_file_of_layout_1_keeps_its_key_and_gains_a_salt() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_at(tmp.path());
        let graph = Graph::new();
        let block = graph.create_block(None, None, "kept").unwrap();
        graph.commit();
        // Made by another device, so that this one's key is not made on the way.
        let maker = Store::new(tmp.path(), Keyring::new(tmp.path().join("maker's keys")));
        maker.init(false).unwrap();
        maker.save(&graph).unwrap();
        // Make the file what the last build made: no salt, and its own key.
        as_layout(&store, 1);
        let own = SecretKey::generate();
        store
            .connect()
            .unwrap()
            .execute(
                "INSERT INTO endpoint_key (id, secret) VALUES (1, ?1)",
                params![own.to_bytes().as_slice()],
            )
            .unwrap();
        assert_eq!(layout(&store), 1);

        assert_eq!(store.key().unwrap().public(), own.public());
        assert_eq!(layout(&store), LAYOUT);
        assert!(salt_in(&store.connect().unwrap()).is_ok());
        assert!(
            !store.keyring.dir().exists(),
            "the device's key was not asked for"
        );
        // Loading it does ask: what the replica edits as is tied to the device.
        assert_eq!(store.load().unwrap().text(block).unwrap(), "kept");
        assert!(store.keyring.dir().exists());
    }

    /// A file of layout 2 has a salt already, which it keeps: the key a graph
    /// derived from it is the same after.
    #[test]
    fn a_file_of_layout_2_keeps_its_salt() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_at(tmp.path());
        store.init(false).unwrap();
        let (key, salt) = (
            store.key().unwrap().public(),
            salt_in(&store.connect().unwrap()).unwrap(),
        );
        as_layout(&store, 2);
        assert_eq!(store.key().unwrap().public(), key);
        assert_eq!(layout(&store), LAYOUT);
        assert_eq!(salt_in(&store.connect().unwrap()).unwrap(), salt);
        assert!(store.load().is_ok());
    }

    /// Two processes opening a file of an earlier layout together convert it
    /// once.
    #[test]
    fn an_earlier_layout_is_converted_once_however_many_open_it() {
        for old in [1, 2] {
            let tmp = tempfile::tempdir().unwrap();
            let store = store_at(tmp.path());
            store.init(false).unwrap();
            as_layout(&store, old);
            let salts: Vec<_> = std::thread::scope(|s| {
                (0..6)
                    .map(|_| {
                        s.spawn(|| {
                            let store = store_at(tmp.path());
                            let db = store.open().unwrap();
                            salt_in(&db).unwrap().to_bytes()
                        })
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
                    .map(|t| t.join().unwrap())
                    .collect()
            });
            assert!(salts.windows(2).all(|w| w[0] == w[1]), "{old}: {salts:?}");
            assert_eq!(layout(&store), LAYOUT);
        }
    }

    /// An earlier layout is only converted if it is exactly that layout.
    #[test]
    fn an_earlier_layout_with_more_in_it_is_refused_and_left_alone() {
        for old in [1, 2] {
            let tmp = tempfile::tempdir().unwrap();
            let store = store_at(tmp.path());
            store.init(false).unwrap();
            as_layout(&store, old);
            store
                .connect()
                .unwrap()
                .execute_batch("CREATE TABLE extra (x)")
                .unwrap();
            assert!(matches!(
                store.load().unwrap_err(),
                StoreError::Unreadable(..)
            ));
            assert_eq!(layout(&store), old, "not converted");
        }
    }

    /// A key that can't be read is reported before anything is written, and
    /// the graph is as it was.
    #[test]
    fn saving_without_a_usable_device_key_changes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_at(tmp.path());
        let graph = store.init(false).unwrap();
        graph.create_block(None, None, "before").unwrap();
        graph.commit();
        store.save(&graph).unwrap();
        let before = fs::read(store.file()).unwrap();

        fs::write(store.keyring.dir().join("device.key"), "damaged\n").unwrap();
        graph.create_block(None, None, "after").unwrap();
        graph.commit();
        assert!(matches!(
            store.save(&graph).unwrap_err(),
            StoreError::DeviceKey(..)
        ));
        assert!(matches!(
            store.key().unwrap_err(),
            StoreError::DeviceKey(..)
        ));
        assert_eq!(fs::read(store.file()).unwrap(), before);
    }

    /// The replica that is kept is one node, however often the graph is opened:
    /// opening a graph a hundred times doesn't add a hundred peers to it.
    #[test]
    fn a_replica_edits_as_one_node_however_often_it_is_opened() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_at(tmp.path());
        let graph = store.init(false).unwrap();
        let node = store.node_id().unwrap();
        assert_eq!(graph.node_id(), node);
        for i in 0..5 {
            let graph = store.load().unwrap();
            assert_eq!(graph.node_id(), node);
            graph
                .create_block(None, None, &format!("round {i}"))
                .unwrap();
            graph.commit();
            store.save(&graph).unwrap();
        }
        let graph = store.load().unwrap();
        assert_eq!(graph.version().nodes(), [node], "one peer, not six");
        assert_eq!(graph.roots().len(), 5);
        assert_eq!(store.node_id().unwrap(), node);
    }

    #[test]
    fn a_blank_replica_edits_as_its_node_too() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_at(tmp.path());
        let graph = store.init(true).unwrap();
        assert_eq!(graph.node_id(), store.node_id().unwrap());
        assert_eq!(graph.version().nodes(), []);
    }

    /// The node ID is what loading applies, from either generation.
    #[test]
    fn loading_from_the_previous_generation_applies_it_too() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_at(tmp.path());
        let graph = store.init(false).unwrap();
        graph.create_block(None, None, "x").unwrap();
        graph.commit();
        store.save(&graph).unwrap();
        store.save(&graph).unwrap();
        store
            .connect()
            .unwrap()
            .execute("UPDATE snapshot SET data = X'00' WHERE id = 1", [])
            .unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.node_id(), store.node_id().unwrap());
    }

    /// A file taken to another device is not that device's replica: it gets a
    /// node ID of its own, and keeps it, while what the first device wrote
    /// under its own stays.
    #[test]
    fn a_graph_taken_to_another_device_gets_a_node_id_of_its_own() {
        let tmp = tempfile::tempdir().unwrap();
        let (first, second) = (tmp.path().join("first"), tmp.path().join("second"));
        let on_first = Store::new(&first, Keyring::new(tmp.path().join("keys 1")));
        let graph = on_first.init(false).unwrap();
        graph.create_block(None, None, "from the first").unwrap();
        graph.commit();
        on_first.save(&graph).unwrap();
        let original = on_first.node_id().unwrap();

        fs::create_dir_all(&second).unwrap();
        fs::copy(on_first.file(), second.join(FILE)).unwrap();
        let on_second = Store::new(&second, Keyring::new(tmp.path().join("keys 2")));
        let moved = on_second.node_id().unwrap();
        assert_ne!(moved, original);
        assert_eq!(on_second.node_id().unwrap(), moved, "and it keeps it");
        assert_eq!(
            on_first.node_id().unwrap(),
            original,
            "the first is as it was"
        );

        let graph = on_second.load().unwrap();
        assert_eq!(graph.node_id(), moved);
        graph.create_block(None, None, "from the second").unwrap();
        graph.commit();
        assert_eq!(graph.version().nodes().len(), 2);
        assert!(graph.version().nodes().contains(&original));
        assert!(graph.version().nodes().contains(&moved));
    }

    /// A copy beside the original, on one device, is the same node: nothing in
    /// the file can say otherwise. Telling them apart is the work of the map of
    /// where a device's graphs are.
    #[test]
    fn a_copy_on_the_same_device_is_the_same_node() {
        let tmp = tempfile::tempdir().unwrap();
        let keys = Keyring::new(tmp.path().join("keys"));
        let one = Store::new(tmp.path().join("one"), keys.clone());
        one.init(false).unwrap();
        let copy = Store::new(tmp.path().join("copy"), keys);
        fs::create_dir_all(copy.dir()).unwrap();
        fs::copy(one.file(), copy.file()).unwrap();
        assert_eq!(copy.node_id().unwrap(), one.node_id().unwrap());
    }

    /// What is in the file is the sealed form: the ID isn't in it as it is.
    #[test]
    fn the_node_id_is_kept_sealed() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_at(tmp.path());
        store.init(false).unwrap();
        let node = store.node_id().unwrap();
        let stored: Vec<u8> = store
            .connect()
            .unwrap()
            .query_row("SELECT sealed FROM node WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stored.len(), 12);
        assert!(!stored.windows(8).any(|w| w == node.get().to_be_bytes()));
        let mask = store
            .keyring
            .key()
            .unwrap()
            .node_mask(&salt_in(&store.connect().unwrap()).unwrap());
        assert_eq!(
            SealedNodeId::from_bytes(stored.try_into().unwrap()).open(&mask),
            Some(node)
        );
    }

    /// A stored value that doesn't open, however it came to be, is not a node
    /// ID to edit as.
    #[test]
    fn a_damaged_node_id_is_replaced_once() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_at(tmp.path());
        store.init(false).unwrap();
        let before = store.node_id().unwrap();
        store
            .connect()
            .unwrap()
            .execute(
                "UPDATE node SET sealed = X'000102030405060708090a0b' WHERE id = 1",
                [],
            )
            .unwrap();
        let after = store.node_id().unwrap();
        assert_ne!(after, before);
        assert_eq!(store.node_id().unwrap(), after);
    }

    /// Processes asking for it together agree on one.
    #[test]
    fn processes_asking_together_agree_on_a_node_id() {
        let tmp = tempfile::tempdir().unwrap();
        store_at(tmp.path()).init(false).unwrap();
        store_at(tmp.path())
            .connect()
            .unwrap()
            .execute("DELETE FROM node", [])
            .unwrap();
        let ids: Vec<_> = std::thread::scope(|s| {
            (0..6)
                .map(|_| s.spawn(|| store_at(tmp.path()).node_id().unwrap()))
                .collect::<Vec<_>>()
                .into_iter()
                .map(|t| t.join().unwrap())
                .collect()
        });
        assert!(ids.windows(2).all(|w| w[0] == w[1]), "{ids:?}");
    }

    /// A graph made before has one too: it needs the device's key for it, and
    /// still keeps its own endpoint key.
    #[test]
    fn a_graph_with_its_own_endpoint_key_has_a_node_id() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_at(tmp.path());
        store.init(false).unwrap();
        let own = SecretKey::generate();
        store
            .connect()
            .unwrap()
            .execute(
                "INSERT INTO endpoint_key (id, secret) VALUES (1, ?1)",
                params![own.to_bytes().as_slice()],
            )
            .unwrap();
        let node = store.node_id().unwrap();
        assert_eq!(store.load().unwrap().node_id(), node);
        assert_eq!(store.key().unwrap().public(), own.public());
    }
}
