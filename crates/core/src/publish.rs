//! What a static site needs from a graph: a page for each entity, addressed
//! by its name.
//!
//! An entity's ID changes whenever a graph is imported afresh, so a link
//! made with it breaks when the site is rebuilt. A name survives, as long as
//! no two entities share one, which a site can't tell apart: every page is a
//! file, and two names that compare equal (see [`normalise_name`]) would be
//! the same file on a file system that ignores case.

use std::collections::BTreeMap;

use quiddity_syntax::normalise_name;

use crate::{Graph, Index};

/// Why a graph can't be published by name.
#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    #[error(transparent)]
    Graph(#[from] crate::Error),
    /// Groups of names that compare equal, each group two or more.
    #[error("these names are used more than once: {}", join_groups(.0))]
    Clashes(Vec<Vec<String>>),
    /// Names that can't be one segment of a path.
    #[error("these names can't be in a path: {}", quote_all(.0))]
    Unroutable(Vec<String>),
}

fn quote_all(names: &[String]) -> String {
    names
        .iter()
        .map(|n| format!("{n:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn join_groups(groups: &[Vec<String>]) -> String {
    groups
        .iter()
        .map(|g| format!("[{}]", quote_all(g)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Whether `name` is one segment of a URL path, with nothing a server or file
/// system would read as moving about.
fn routable(name: &str) -> bool {
    !(name.contains(['/', '\\'])
        || name == "."
        || name == ".."
        || name.chars().any(char::is_control))
}

/// The name of every entity, sorted, if each is a path segment of its own.
/// Entities merged into another as duplicates aren't counted.
pub fn page_names(graph: &Graph) -> Result<Vec<String>, PublishError> {
    let index = Index::build(graph)?;
    let mut by_key: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entity in index.entities() {
        by_key
            .entry(normalise_name(&entity.name))
            .or_default()
            .push(entity.name.clone());
    }
    let mut clashes: Vec<Vec<String>> = by_key
        .values()
        .filter(|names| names.len() > 1)
        .cloned()
        .collect();
    if !clashes.is_empty() {
        clashes.iter_mut().for_each(|g| g.sort());
        return Err(PublishError::Clashes(clashes));
    }
    let mut names: Vec<String> = by_key.into_values().flatten().collect();
    let bad: Vec<String> = names.iter().filter(|n| !routable(n)).cloned().collect();
    if !bad.is_empty() {
        return Err(PublishError::Unroutable(bad));
    }
    names.sort();
    Ok(names)
}
