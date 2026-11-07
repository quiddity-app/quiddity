//! A graph on disk: a directory holding the graph's snapshot and the
//! device's secret key.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use iroh::SecretKey;
use jiff::{SignedDuration, Timestamp};
use tracing::instrument;
use quiddity_core::{Graph, Version};

const SNAPSHOT: &str = "graph.loro";
const KEY: &str = "device.key";

/// How long a device can go without saving before it stops holding back
/// other devices' compaction.
pub const ACTIVE: SignedDuration = SignedDuration::from_hours(24 * 30);

#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    #[instrument(level = "debug")]
    pub fn new(dir: impl Into<PathBuf> + std::fmt::Debug) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Creates the directory with an empty graph (or, with `blank`, an empty
    /// replica for a device that will sync an existing graph).
    #[instrument(err(level = "error"))]
    pub fn init(&self, blank: bool) -> Result<Graph, crate::StoreError> {
        if self.dir.join(SNAPSHOT).exists() {
            return Err(crate::StoreError::Exists(self.dir.clone()));
        }
        fs::create_dir_all(&self.dir)?;
        let graph = if blank { Graph::blank() } else { Graph::new() };
        self.save(&graph)?;
        Ok(graph)
    }

    #[instrument(err(level = "error"))]
    pub fn load(&self) -> Result<Graph, crate::StoreError> {
        let bytes = fs::read(self.dir.join(SNAPSHOT))?;
        Ok(Graph::from_snapshot(&bytes)?)
    }

    /// Saves atomically: a crash mid-save leaves the previous snapshot.
    ///
    /// The saved graph records that this device has stored it, and keeps
    /// only the history some device might still need. A blank replica
    /// records nothing: it has yet to join.
    #[instrument(err(level = "error"))]
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
        let tmp = self.dir.join(format!("{SNAPSHOT}.tmp"));
        fs::write(&tmp, snapshot)?;
        fs::rename(tmp, self.dir.join(SNAPSHOT))?;
        Ok(())
    }

    /// The device's key, created on first use. Its public half is the
    /// device's ID.
    #[instrument(err(level = "error"))]
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
                let key = SecretKey::generate();
                let mut options = fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
                io::Write::write_all(&mut options.open(&path)?, &key.to_bytes())?;
                Ok(key)
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

    #[test]
    fn the_device_key_is_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        let key = store.key().unwrap();
        assert_eq!(store.key().unwrap().public(), key.public());

        fs::write(tmp.path().join(KEY), b"short").unwrap();
        assert_eq!(store.key().unwrap_err().kind(), io::ErrorKind::InvalidData);
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
