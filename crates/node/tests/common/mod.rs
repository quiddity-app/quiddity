//! What the tests of `quiddity-node` share.

use std::path::{Path, PathBuf};

use quiddity_node::keyring::Keyring;
use quiddity_node::store::Store;

/// Where the device that keeps `graph` keeps its key: beside it, and its own,
/// so that a test never touches the real key and each graph is a device.
pub fn keys(graph: &Path) -> PathBuf {
    let mut name = graph.file_name().unwrap().to_owned();
    name.push(".config");
    graph.with_file_name(name)
}

/// The store of the graph in `dir`, for the device that keeps it.
pub fn store(dir: &Path) -> Store {
    Store::new(dir, Keyring::new(keys(dir)))
}
