//! A graph on disk: a directory holding the graph's snapshot and the
//! device's secret key.

use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use iroh::SecretKey;
use jiff::{SignedDuration, Timestamp};
use quiddity_core::{Graph, Version};
use tracing::{debug, info, instrument, warn};

const SNAPSHOT: &str = "graph.loro";
const BACKUP: &str = "graph.loro.bak";
const KEY: &str = "device.key";

/// Makes temporary names unique within the process; the process ID covers
/// other processes sharing a directory.
static TEMPORARIES: AtomicU64 = AtomicU64::new(0);

fn temporary(dir: &Path, name: &str) -> PathBuf {
    let n = TEMPORARIES.fetch_add(1, Ordering::Relaxed);
    dir.join(format!("{name}.{}.{n}.tmp", std::process::id()))
}

/// Writes `bytes` to `path` so that, after a crash, `path` holds either what
/// it held before or all of `bytes`: the data is flushed before the rename,
/// and the directory after it.
fn write_durably(dir: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = temporary(dir, &path.file_name().unwrap_or_default().to_string_lossy());
    let written = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written?;
    sync_dir(dir)
}

/// Creates `path` holding `bytes`, only if it doesn't exist: written whole
/// under another name, then hard-linked into place, so a crash leaves no file
/// or a complete one, and a second creator can't replace the first's. Says
/// whether it created it.
fn write_new(dir: &Path, path: &Path, bytes: &[u8]) -> io::Result<bool> {
    let tmp = temporary(dir, &path.file_name().unwrap_or_default().to_string_lossy());
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&tmp)?;
    let linked = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| fs::hard_link(&tmp, path));
    let _ = fs::remove_file(&tmp);
    match linked {
        Ok(()) => {
            sync_dir(dir)?;
            Ok(true)
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e),
    }
}

/// Flushes a directory's entries, so a rename survives power loss. Windows
/// can't open a directory to do so, and doesn't need it to.
fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    fs::File::open(dir)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

/// How long a device can go without saving before it stops holding back
/// other devices' compaction.
pub const ACTIVE: SignedDuration = SignedDuration::from_hours(24 * 30);

#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
    /// Serialises saves, which come from the command thread and the
    /// save-on-change task at once. Clones share it.
    saving: Arc<Mutex<()>>,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            saving: Arc::default(),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Creates the directory with an empty graph (or, with `blank`, an empty
    /// replica for a device that will sync an existing graph).
    #[instrument(level = "debug", skip_all, fields(dir = %self.dir.display()))]
    pub fn init(&self, blank: bool) -> Result<Graph, crate::StoreError> {
        if self.dir.join(SNAPSHOT).exists() {
            return Err(crate::StoreError::Exists(self.dir.clone()));
        }
        fs::create_dir_all(&self.dir)?;
        let graph = if blank { Graph::blank() } else { Graph::new() };
        self.save(&graph)?;
        Ok(graph)
    }

    #[instrument(level = "debug", skip_all, fields(dir = %self.dir.display()))]
    pub fn load(&self) -> Result<Graph, crate::StoreError> {
        let bytes = match fs::read(self.dir.join(SNAPSHOT)) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(crate::StoreError::Missing(self.dir.clone()));
            }
            Err(e) => return Err(e.into()),
        };
        debug!(bytes = bytes.len(), "loading the snapshot");
        match Graph::from_snapshot(&bytes) {
            Ok(graph) => Ok(graph),
            Err(e) => {
                // A snapshot that can't be decoded is never replaced by an
                // empty graph, but the previous generation is better than
                // refusing to start. It may be behind what peers know this
                // device stored: they will say so.
                let Ok(previous) = fs::read(self.dir.join(BACKUP)) else {
                    return Err(e.into());
                };
                let graph = Graph::from_snapshot(&previous).map_err(|_| e)?;
                warn!("the snapshot couldn't be loaded: using the previous one");
                Ok(graph)
            }
        }
    }

    /// Saves atomically and durably: a crash mid-save, even a power loss,
    /// leaves the previous snapshot, which is also kept as `graph.loro.bak`.
    ///
    /// The saved graph records that this device has stored it, and keeps
    /// only the history some device might still need. A blank replica
    /// records nothing: it has yet to join.
    #[instrument(level = "debug", skip_all, fields(dir = %self.dir.display()))]
    pub fn save(&self, graph: &Graph) -> Result<(), crate::StoreError> {
        let snapshot = if graph.version() == Version::default() {
            graph.snapshot()?
        } else {
            let now = Timestamp::now();
            graph.record_stored(&self.key()?.public().to_string(), now)?;
            match graph.compaction_point(now, ACTIVE) {
                Some(at) => graph.shallow_snapshot(&at)?,
                None => graph.snapshot()?,
            }
        };
        debug!(bytes = snapshot.len(), "saving the snapshot");
        let _saving = self.saving.lock().unwrap_or_else(|e| e.into_inner());
        let path = self.dir.join(SNAPSHOT);
        // Best effort: the current file is intact whatever happens to the copy.
        if path.exists()
            && let Err(e) = fs::copy(&path, self.dir.join(BACKUP))
        {
            warn!(error = %e, "couldn't keep the previous snapshot");
        }
        write_durably(&self.dir, &path, &snapshot)?;
        Ok(())
    }

    /// The device's key, created on first use. Its public half is the
    /// device's ID.
    #[instrument(level = "debug", skip_all, fields(dir = %self.dir.display()))]
    pub fn key(&self) -> io::Result<SecretKey> {
        let path = self.dir.join(KEY);
        match fs::read(&path) {
            Ok(bytes) => {
                let bytes: [u8; 32] = bytes
                    .try_into()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad device key"))?;
                Ok(SecretKey::from_bytes(&bytes))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                info!(path = %path.display(), "creating this device's key");
                let key = SecretKey::generate();
                if write_new(&self.dir, &path, &key.to_bytes())? {
                    Ok(key)
                } else {
                    // Another creator got there first: theirs stands.
                    self.key()
                }
            }
            Err(e) => Err(e),
        }
    }
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

        let saved = fs::read(tmp.path().join(SNAPSHOT)).unwrap();
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

    /// Each save leaves the one before it beside the snapshot, no temporary
    /// files are left behind, and a snapshot that can't be read falls back
    /// to the previous one.
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

        let mut names: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, [KEY, SNAPSHOT, BACKUP]);

        fs::write(tmp.path().join(SNAPSHOT), b"").unwrap();
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
        fs::write(tmp.path().join(SNAPSHOT), b"").unwrap();
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

    #[test]
    fn the_device_key_is_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        let key = store.key().unwrap();
        assert_eq!(store.key().unwrap().public(), key.public());

        fs::write(tmp.path().join(KEY), b"short").unwrap();
        assert_eq!(store.key().unwrap_err().kind(), io::ErrorKind::InvalidData);
    }

    /// A file that exists is left alone, and says so; nothing else is left
    /// behind. Other failures are errors, not "it exists".
    #[test]
    fn a_new_file_never_replaces_one() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("file");
        assert!(write_new(tmp.path(), &path, b"first").unwrap());
        assert!(!write_new(tmp.path(), &path, b"second").unwrap());
        assert_eq!(fs::read(&path).unwrap(), b"first");
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1);

        let missing = tmp.path().join("no such dir").join("file");
        assert!(write_new(tmp.path(), &missing, b"x").is_err());
    }

    /// A key that can't be read is reported as it is, and never replaced.
    #[test]
    fn an_unreadable_key_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join(KEY)).unwrap();
        let err = Store::new(tmp.path()).key().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::IsADirectory);
    }
}
