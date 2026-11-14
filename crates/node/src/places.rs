//! Where a device's graphs are, by the node ID each edits as.
//!
//! A graph is a file, and a file can be copied: by hand, by a backup, or by a
//! service that syncs folders. A copy beside the original on one device is the
//! same device holding the same node ID, and two replicas editing as one node
//! make different edits with the same ID, which Loro keeps one of without a
//! word. So each device keeps a table of which node ID it has seen where, in
//! its own configuration directory and never in a graph (a graph that is
//! copied would copy it too). A graph that is opened at another place than the
//! one its node ID was seen at is either moved, if the old place no longer holds
//! it, or copied, if it still does, and a copy takes a node ID of its own
//! before it edits anything.

use std::path::{Path, PathBuf};
use std::time::Duration;

use quiddity_core::NodeId;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

const FILE: &str = "places.sqlite3";

/// How long to wait for another process that is writing the table.
const BUSY: Duration = Duration::from_secs(5);

/// What opening a graph at a place made of its node ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claim {
    /// A node ID not seen before: it is here now.
    New,
    /// Seen here already.
    Same,
    /// Seen somewhere else, which no longer holds it: the graph was moved, and
    /// it is here now.
    Moved,
    /// Seen somewhere else, which still holds it: this is a copy of that one,
    /// and the table is as it was. The copy must edit as another node.
    Copy { of: PathBuf },
}

/// The table, in a file of its own.
#[derive(Debug, Clone)]
pub struct Places {
    file: PathBuf,
}

impl Places {
    /// The table kept in `dir`, which is made if there is none.
    pub fn new(dir: &Path) -> Self {
        Self {
            file: dir.join(FILE),
        }
    }

    fn open(&self) -> rusqlite::Result<Connection> {
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        }
        let db = Connection::open(&self.file)?;
        db.busy_timeout(BUSY)?;
        db.pragma_update(None, "synchronous", "FULL")?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS place (node TEXT PRIMARY KEY, path TEXT NOT NULL) STRICT",
        )?;
        Ok(db)
    }

    /// Says what opening the graph at `here` makes of `node`, and records it
    /// unless it is a copy.
    ///
    /// `holds` says whether the graph at another place still has that node ID.
    /// It is asked while the table is held for writing, so that two copies
    /// opened together can't both be the first: one finds the other's place, and
    /// is a copy.
    pub fn claim(
        &self,
        node: NodeId,
        here: &Path,
        holds: impl FnOnce(&Path) -> bool,
    ) -> rusqlite::Result<Claim> {
        let here = text(here)?;
        let mut db = self.open()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let key = node.to_string();
        let seen: Option<String> = tx
            .query_row("SELECT path FROM place WHERE node = ?1", [&key], |r| {
                r.get(0)
            })
            .optional()?;
        let claim = match seen {
            None => Claim::New,
            Some(seen) if seen == here => Claim::Same,
            Some(seen) => {
                let seen = PathBuf::from(seen);
                if holds(&seen) {
                    return Ok(Claim::Copy { of: seen });
                }
                Claim::Moved
            }
        };
        if claim != Claim::Same {
            tx.execute(
                "INSERT INTO place (node, path) VALUES (?1, ?2)
                 ON CONFLICT (node) DO UPDATE SET path = excluded.path",
                params![key, here],
            )?;
        }
        tx.commit()?;
        Ok(claim)
    }

    /// Records that `node` is at `here`, whatever was there.
    pub fn record(&self, node: NodeId, here: &Path) -> rusqlite::Result<()> {
        self.open()?.execute(
            "INSERT INTO place (node, path) VALUES (?1, ?2)
             ON CONFLICT (node) DO UPDATE SET path = excluded.path",
            params![node.to_string(), text(here)?],
        )?;
        Ok(())
    }

    /// Where `node` was last seen.
    pub fn lookup(&self, node: NodeId) -> rusqlite::Result<Option<PathBuf>> {
        let seen: Option<String> = self
            .open()?
            .query_row(
                "SELECT path FROM place WHERE node = ?1",
                [node.to_string()],
                |r| r.get(0),
            )
            .optional()?;
        Ok(seen.map(PathBuf::from))
    }
}

/// A path as text. One that isn't text can't be kept, and is refused rather
/// than changed: a changed path would name another place.
fn text(path: &Path) -> rusqlite::Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| rusqlite::Error::InvalidPath(path.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn places() -> (tempfile::TempDir, Places) {
        let tmp = tempfile::tempdir().unwrap();
        let places = Places::new(&tmp.path().join("config"));
        (tmp, places)
    }

    const NODE: NodeId = NodeId::new(7);

    #[test]
    fn a_node_is_new_once_and_then_the_same() {
        let (_tmp, places) = places();
        let here = Path::new("/graphs/one");
        assert_eq!(places.claim(NODE, here, |_| true).unwrap(), Claim::New);
        assert_eq!(places.claim(NODE, here, |_| true).unwrap(), Claim::Same);
        assert_eq!(places.lookup(NODE).unwrap(), Some(here.to_owned()));
        // Another node is not that one.
        assert_eq!(
            places.claim(NodeId::new(8), here, |_| true).unwrap(),
            Claim::New
        );
        assert_eq!(places.lookup(NodeId::new(9)).unwrap(), None);
    }

    #[test]
    fn a_graph_whose_old_place_has_gone_has_moved() {
        let (_tmp, places) = places();
        places.claim(NODE, Path::new("/old"), |_| true).unwrap();
        let asked = std::cell::RefCell::new(None);
        let claim = places
            .claim(NODE, Path::new("/new"), |old| {
                *asked.borrow_mut() = Some(old.to_owned());
                false
            })
            .unwrap();
        assert_eq!(claim, Claim::Moved);
        assert_eq!(
            asked.into_inner(),
            Some(PathBuf::from("/old")),
            "it asked about the old"
        );
        assert_eq!(places.lookup(NODE).unwrap(), Some("/new".into()));
    }

    #[test]
    fn a_graph_whose_old_place_still_holds_it_is_a_copy_and_changes_nothing() {
        let (_tmp, places) = places();
        places
            .claim(NODE, Path::new("/original"), |_| true)
            .unwrap();
        let claim = places.claim(NODE, Path::new("/copy"), |_| true).unwrap();
        assert_eq!(
            claim,
            Claim::Copy {
                of: "/original".into()
            }
        );
        assert_eq!(places.lookup(NODE).unwrap(), Some("/original".into()));
    }

    #[test]
    fn recording_replaces_what_was_there() {
        let (_tmp, places) = places();
        places.record(NODE, Path::new("/a")).unwrap();
        places.record(NODE, Path::new("/b")).unwrap();
        assert_eq!(places.lookup(NODE).unwrap(), Some("/b".into()));
    }

    /// Copies opened together: one is first, and every other finds its place
    /// and is a copy. None is first as well, which would be two replicas as one
    /// node.
    #[test]
    fn copies_opened_together_cannot_all_be_first() {
        let (_tmp, places) = places();
        let claims: Vec<_> = std::thread::scope(|s| {
            (0..8)
                .map(|i| {
                    let places = places.clone();
                    s.spawn(move || {
                        places
                            .claim(NODE, Path::new(&format!("/copy/{i}")), |_| true)
                            .unwrap()
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|t| t.join().unwrap())
                .collect()
        });
        let first = claims.iter().filter(|c| **c == Claim::New).count();
        let copies = claims
            .iter()
            .filter(|c| matches!(c, Claim::Copy { .. }))
            .count();
        assert_eq!((first, copies), (1, 7), "{claims:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_path_that_is_not_text_is_refused_and_not_changed() {
        use std::os::unix::ffi::OsStrExt;
        let (_tmp, places) = places();
        let odd = Path::new(std::ffi::OsStr::from_bytes(b"/graphs/\xff"));
        assert!(matches!(
            places.claim(NODE, odd, |_| true),
            Err(rusqlite::Error::InvalidPath(_))
        ));
        assert!(places.record(NODE, odd).is_err());
        assert_eq!(places.lookup(NODE).unwrap(), None);
    }

    #[test]
    fn a_table_that_is_not_one_is_an_error_not_a_panic() {
        let (tmp, places) = places();
        std::fs::create_dir_all(tmp.path().join("config")).unwrap();
        std::fs::write(
            tmp.path().join("config").join(FILE),
            b"this is not a database, but is long enough to look like one",
        )
        .unwrap();
        assert!(places.claim(NODE, Path::new("/a"), |_| true).is_err());
        assert!(places.lookup(NODE).is_err());
    }
}
