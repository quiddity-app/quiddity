//! The graphs on this device: one directory each, under `graphs/` in the app's
//! data directory, and a note of which one is open.
//!
//! It knows nothing about Tauri or about what is in a graph, only where they
//! are and what they may be called, so it is tested on its own.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::session::Error;

/// What the one graph of an app with a single graph is called, and so what the
/// graph made before there could be several becomes.
const DEFAULT: &str = "Quiddity";
/// Where graphs are, in the data directory.
const GRAPHS: &str = "graphs";
/// Where the graph made before there could be several was.
const LEGACY: &str = "graph";
/// Which graph is open, by name, in the data directory.
const CURRENT: &str = "current-graph";
/// A name this long is a mistake, and a path with it in may be too long.
const LONGEST: usize = 64;

/// The graphs, for the UI's picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Graphs {
    pub current: String,
    /// In name order, ignoring case.
    pub all: Vec<String>,
    /// Whether there is a choice: `QUIDDITY_GRAPH` names one graph and no
    /// others can be made or opened while it does.
    pub switchable: bool,
}

#[derive(Debug, Clone)]
pub struct Library {
    root: PathBuf,
    /// A graph named from outside (`QUIDDITY_GRAPH`), which is the only one.
    fixed: Option<PathBuf>,
}

impl Library {
    #[must_use]
    pub fn new(root: PathBuf, fixed: Option<PathBuf>) -> Self {
        Self { root, fixed }
    }

    fn graphs(&self) -> PathBuf {
        self.root.join(GRAPHS)
    }

    /// The directory of the graph called `name`, which may not be there.
    #[must_use]
    pub fn dir(&self, name: &str) -> PathBuf {
        self.graphs().join(name)
    }

    /// The name a directory is listed under: the last part of its path.
    #[must_use]
    pub fn name_of(dir: &Path) -> String {
        let name = dir.file_stem().or_else(|| dir.file_name());
        name.map_or_else(|| DEFAULT.to_owned(), |n| n.to_string_lossy().into_owned())
    }

    /// Moves the graph made before there could be several in with the others.
    /// A rename, so it is whole or not done; nothing is copied or deleted.
    fn adopt_legacy(&self) -> io::Result<()> {
        let old = self.root.join(LEGACY);
        let new = self.dir(DEFAULT);
        if old.is_dir() && !new.exists() {
            fs::create_dir_all(self.graphs())?;
            fs::rename(old, new)?;
        }
        Ok(())
    }

    /// The directory of the graph to open at startup: the one last chosen if it
    /// is still there, otherwise the first there is, otherwise a new one's.
    pub fn start(&self) -> Result<PathBuf, Error> {
        if let Some(dir) = &self.fixed {
            return Ok(dir.clone());
        }
        self.adopt_legacy()?;
        let chosen = fs::read_to_string(self.root.join(CURRENT))
            .ok()
            .map(|name| name.trim().to_owned())
            .filter(|name| valid(name).is_ok() && self.dir(name).is_dir());
        let name = match chosen {
            Some(name) => name,
            None => self
                .names()?
                .into_iter()
                .next()
                .unwrap_or_else(|| DEFAULT.to_owned()),
        };
        Ok(self.dir(&name))
    }

    fn names(&self) -> Result<Vec<String>, Error> {
        let mut names = match fs::read_dir(self.graphs()) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|name| valid(name).is_ok())
                .collect(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        names.sort_by_key(|name| name.to_lowercase());
        Ok(names)
    }

    /// The graphs there are, given that `current` is the one open.
    pub fn list(&self, current: &Path) -> Result<Graphs, Error> {
        let current = Self::name_of(current);
        if self.fixed.is_some() {
            return Ok(Graphs {
                all: vec![current.clone()],
                current,
                switchable: false,
            });
        }
        let mut all = self.names()?;
        if !all.contains(&current) {
            all.push(current.clone());
        }
        Ok(Graphs {
            current,
            all,
            switchable: true,
        })
    }

    /// The directory of an existing graph, for opening it.
    pub fn existing(&self, name: &str) -> Result<PathBuf, Error> {
        self.switchable()?;
        valid(name)?;
        let dir = self.dir(name);
        if dir.is_dir() {
            Ok(dir)
        } else {
            Err(Error::NoSuchGraph(name.to_owned()))
        }
    }

    /// The directory for a new graph called `name`, which is not made yet.
    /// A name that differs from another only in case is taken, since some
    /// file systems would make them one directory.
    pub fn fresh(&self, name: &str) -> Result<(String, PathBuf), Error> {
        self.switchable()?;
        let name = name.trim();
        valid(name)?;
        if self
            .names()?
            .iter()
            .any(|n| n.to_lowercase() == name.to_lowercase())
        {
            return Err(Error::GraphExists(name.to_owned()));
        }
        Ok((name.to_owned(), self.dir(name)))
    }

    fn switchable(&self) -> Result<(), Error> {
        if self.fixed.is_some() {
            Err(Error::OneGraph)
        } else {
            Ok(())
        }
    }

    /// Notes which graph to open next time.
    pub fn choose(&self, name: &str) -> Result<(), Error> {
        if self.fixed.is_some() {
            return Ok(());
        }
        fs::create_dir_all(&self.root)?;
        // Written beside and renamed, so a crash leaves the old choice whole.
        let staging = self.root.join(format!("{CURRENT}.new"));
        fs::write(&staging, name)?;
        fs::rename(staging, self.root.join(CURRENT))?;
        Ok(())
    }
}

/// Whether `name` can be a directory's name on any system, and is something
/// a person can read in a menu.
fn valid(name: &str) -> Result<(), Error> {
    let bad = name.is_empty()
        || name != name.trim()
        || name.chars().count() > LONGEST
        || name.starts_with('.')
        || name.ends_with('.')
        || name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        });
    if bad {
        Err(Error::BadGraphName(name.to_owned()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> (tempfile::TempDir, Library) {
        let root = tempfile::tempdir().unwrap();
        let library = Library::new(root.path().to_owned(), None);
        (root, library)
    }

    #[test]
    fn a_new_device_starts_with_the_default_graph() {
        let (_root, library) = library();
        let dir = library.start().unwrap();
        assert_eq!(Library::name_of(&dir), DEFAULT);
        // It is listed even though nothing has made it yet.
        let graphs = library.list(&dir).unwrap();
        assert_eq!(graphs.all, [DEFAULT]);
        assert!(graphs.switchable);
    }

    #[test]
    fn the_graph_made_before_there_could_be_several_is_adopted() {
        let (root, library) = library();
        fs::create_dir_all(root.path().join(LEGACY)).unwrap();
        fs::write(root.path().join(LEGACY).join("graph.quiddity"), "x").unwrap();
        let dir = library.start().unwrap();
        assert_eq!(dir, library.dir(DEFAULT));
        assert!(dir.join("graph.quiddity").is_file());
        assert!(!root.path().join(LEGACY).exists());
    }

    #[test]
    fn the_choice_is_remembered_until_the_graph_goes() {
        let (_root, library) = library();
        for name in ["Work", "Home"] {
            fs::create_dir_all(library.dir(name)).unwrap();
        }
        library.choose("Work").unwrap();
        assert_eq!(library.start().unwrap(), library.dir("Work"));
        fs::remove_dir(library.dir("Work")).unwrap();
        assert_eq!(library.start().unwrap(), library.dir("Home"));
    }

    #[test]
    fn graphs_are_listed_in_name_order_whatever_the_case() {
        let (_root, library) = library();
        for name in ["b", "A", "c"] {
            fs::create_dir_all(library.dir(name)).unwrap();
        }
        assert_eq!(
            library.list(&library.dir("b")).unwrap().all,
            ["A", "b", "c"]
        );
    }

    #[test]
    fn names_that_are_unsafe_or_taken_are_refused() {
        let (_root, library) = library();
        fs::create_dir_all(library.dir("Work")).unwrap();
        for bad in [
            "", " ", "..", ".hidden", "a/b", "a\\b", "x:y", "a\tb", "dot.",
        ] {
            assert!(library.fresh(bad).is_err(), "{bad:?}");
        }
        assert!(library.fresh(&"x".repeat(LONGEST + 1)).is_err());
        assert!(library.fresh("work").is_err());
        assert_eq!(library.fresh("  Play ").unwrap().0, "Play");
        assert!(library.existing("Missing").is_err());
        assert!(library.existing("../Work").is_err());
        assert!(library.existing("Work").is_ok());
    }

    #[test]
    fn a_named_graph_is_the_only_one() {
        let root = tempfile::tempdir().unwrap();
        let library = Library::new(root.path().to_owned(), Some("/somewhere/Notes".into()));
        let dir = library.start().unwrap();
        let graphs = library.list(&dir).unwrap();
        assert_eq!(
            (graphs.current.as_str(), graphs.switchable),
            ("Notes", false)
        );
        assert!(library.fresh("Other").is_err());
        assert!(library.existing("Notes").is_err());
        library.choose("Notes").unwrap();
        assert!(!root.path().join(CURRENT).exists());
    }
}
