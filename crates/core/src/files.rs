//! Exporting a whole graph to a directory of Markdown files, and importing
//! one, either Quiddity's own export or a Logseq graph.
//!
//! ```text
//! export/
//!   quiddity.json        marks the directory as a Quiddity export
//!   quiddity.toml        the graph's settings, if any are set
//!   entities/Jane.md
//!   entities/Jane 2.md   same name, different entity
//!   journals/2026-10-25.md
//! ```

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use jiff::civil::Date;
use quiddity_syntax::{Link, Value, format_property_line, normalise_name, parse_property_line};
use tracing::{debug, instrument, trace, warn};

use crate::merge::is_spent_duplicate;
use crate::outline::{Indent, Outline, export_entity, import_outline, parse_outline};
use crate::{BlockId, Graph, Identity, Index, Result, Settings};

pub const MANIFEST: &str = "quiddity.json";
const MANIFEST_BODY: &str = "{\"format\": \"quiddity\", \"version\": 1}\n";
/// The graph's settings: see [`Settings`].
pub const SETTINGS: &str = "quiddity.toml";

/// Writes every entity to `dir`, which must be empty or not yet exist.
/// Returns the number of files written.
#[instrument(level = "debug", skip(graph), fields(dir = %dir.display()))]
pub fn export_dir(graph: &Graph, dir: &Path) -> Result<usize> {
    if dir.exists() && fs::read_dir(dir)?.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} is not empty", dir.display()),
        )
        .into());
    }
    fs::create_dir_all(dir.join("entities"))?;
    fs::create_dir_all(dir.join("journals"))?;
    fs::write(dir.join(MANIFEST), MANIFEST_BODY)?;
    let settings = graph.settings();
    if !settings.is_empty() {
        fs::write(dir.join(SETTINGS), settings.to_toml())?;
    }

    let index = Index::build(graph)?;
    let mut used = HashSet::new();
    let mut written = 0;
    for entity in graph.roots() {
        if is_spent_duplicate(graph, &index, entity)? {
            continue;
        }
        let info = index.entity(entity).expect("every root is indexed");
        let (sub, stem) = match info.identity() {
            Some(Identity::Journal(date)) => ("journals", date.to_string()),
            _ => ("entities", file_stem(&info.name)),
        };
        let path = unique_path(&dir.join(sub), &stem, &mut used);
        trace!(path = %path.display(), "exporting");
        fs::write(path, export_entity(graph, entity)?)?;
        written += 1;
    }
    debug!(entities = written, "exported");
    Ok(written)
}

/// Imports every Markdown file under `dir` as an entity. Files in a
/// `journals` directory named for a date (`2026-10-25.md`, or Logseq's
/// `2026_10_25.md`) become journal pages. A `quiddity.toml` sets the graph's
/// settings; those it doesn't mention are left as they were.
///
/// In a Logseq graph (no `quiddity.json`), a page name with a `/` in it is a
/// namespace: `Places/Scotland` is a child of `Places`. Logseq writes the
/// slash as `___` in file names, so that is read back as `/` when the file has
/// no `title::`. Each namespaced page gets a `parent` property naming the page
/// above it, ancestors that have no page of their own are made, and `parent`
/// is defined, with `children` as its inverse, unless the graph has it already.
#[instrument(level = "debug", skip(graph), fields(dir = %dir.display()))]
pub fn import_dir(graph: &Graph, dir: &Path) -> Result<Vec<BlockId>> {
    // Name the directory: the error from reading it doesn't.
    if !dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is not a directory", dir.display()),
        )
        .into());
    }
    // Read first, so that a mistake in it stops the import before anything
    // has been added.
    let settings = match fs::read_to_string(dir.join(SETTINGS)) {
        Ok(text) => Some(Settings::from_toml(&text)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let logseq = !dir.join(MANIFEST).is_file();
    let indent = if logseq { Indent::Auto } else { Indent::Tabs };
    let mut files = Vec::new();
    collect_markdown(dir, &mut files)?;
    files.sort();
    if files.is_empty() {
        warn!("no Markdown files to import");
    }
    let mut ids = Vec::new();
    let mut ancestors = Vec::new();
    for path in files {
        trace!(path = %path.display(), "importing");
        let mut outline = parse_outline(&fs::read_to_string(&path)?, indent);
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let in_journals = path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|n| n == "journals");
        if in_journals && let Ok(date) = stem.replace('_', "-").parse::<Date>() {
            if !has_property(&outline, "type") {
                let date = date.to_string();
                let mut lines = vec![
                    format_property_line("type", Some(&Value::Text("journal".into()))),
                    format_property_line("date", Some(&Value::Text(date.clone()))),
                ];
                lines.extend(outline.properties.take());
                outline.properties = Some(lines.join("\n"));
                outline.title.get_or_insert(date);
            }
        } else if logseq {
            let name = (outline.title.clone()).unwrap_or_else(|| stem.replace("___", "/"));
            if let Some(parent) = namespace_parent(&name) {
                add_parent(&mut outline, &parent);
                ancestors.push(parent);
            }
            outline.title = Some(name);
        }
        ids.push(import_outline(graph, &outline, &stem)?);
    }
    if !ancestors.is_empty() {
        ids.extend(import_namespaces(graph, ancestors)?);
    }
    if let Some(settings) = settings {
        debug!("applying the settings");
        graph.apply_settings(&settings)?;
    }
    debug!(entities = ids.len(), "imported");
    Ok(ids)
}

/// Whether the outline's properties block has a line for `key`.
fn has_property(outline: &Outline, key: &str) -> bool {
    outline.properties.as_deref().is_some_and(|p| {
        p.lines()
            .any(|l| parse_property_line(l).is_some_and(|(k, _)| k.eq_ignore_ascii_case(key)))
    })
}

/// The namespace above a page name: `Places/Scotland` is under `Places`.
fn namespace_parent(name: &str) -> Option<String> {
    let (parent, child) = name.rsplit_once('/')?;
    let parent = parent.trim();
    (!parent.is_empty() && !child.trim().is_empty()).then(|| parent.to_owned())
}

/// Adds `parent:: [[parent]]` to the outline, unless it says its own.
fn add_parent(outline: &mut Outline, parent: &str) {
    if has_property(outline, "parent") {
        return;
    }
    let line = format_property_line(
        "parent",
        Some(&Value::Link(Link {
            name: parent.to_owned(),
            qualifiers: vec![],
        })),
    );
    outline.properties = Some(match outline.properties.take() {
        Some(p) => format!("{p}\n{line}"),
        None => line,
    });
}

/// Makes the namespaces that were named as parents but have no page, and the
/// `parent` property if the graph doesn't define it.
fn import_namespaces(graph: &Graph, mut pending: Vec<String>) -> Result<Vec<BlockId>> {
    let index = Index::build(graph)?;
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    while let Some(name) = pending.pop() {
        if !seen.insert(normalise_name(&name)) || !index.named(&name).is_empty() {
            continue;
        }
        let mut outline = Outline {
            title: Some(name.clone()),
            properties: None,
            blocks: Vec::new(),
        };
        if let Some(parent) = namespace_parent(&name) {
            add_parent(&mut outline, &parent);
            pending.push(parent);
        }
        ids.push(import_outline(graph, &outline, &name)?);
    }
    if index.property_entity("parent").is_none() {
        let outline = Outline {
            title: Some("parent".into()),
            properties: Some("type:: property\n##inverse(children)".into()),
            blocks: Vec::new(),
        };
        ids.push(import_outline(graph, &outline, "parent")?);
    }
    Ok(ids)
}

fn collect_markdown(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Hidden directories, and Logseq's config and backups.
        if name.starts_with('.') || name == "logseq" {
            trace!(path = %path.display(), "skipping");
            continue;
        }
        if entry.file_type()?.is_dir() {
            collect_markdown(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            out.push(path);
        }
    }
    Ok(())
}

/// A file name for an entity. Names needn't be unique or file-system safe;
/// the real name is kept in the file's `title::`.
fn file_stem(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_control() || r#"/\:*?"<>|"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(120)
        .collect();
    let safe = safe
        .trim()
        .trim_end_matches(['.', ' '])
        .trim_start_matches('.');
    let reserved = matches!(
        safe.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    match safe {
        "" => "untitled".to_owned(),
        s if reserved => format!("{s}_"),
        s => s.to_owned(),
    }
}

/// `dir/stem.md`, or `dir/stem 2.md` and so on if taken. Compared
/// case-insensitively, for case-insensitive file systems.
fn unique_path(dir: &Path, stem: &str, used: &mut HashSet<String>) -> PathBuf {
    let mut n = 1;
    loop {
        let candidate = if n == 1 {
            stem.to_owned()
        } else {
            format!("{stem} {n}")
        };
        let key = format!("{}/{}", dir.display(), candidate.to_lowercase());
        if used.insert(key) {
            return dir.join(format!("{candidate}.md"));
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Importing from somewhere that isn't there says where.
    #[test]
    fn importing_a_missing_directory_names_it() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nothing");
        let err = import_dir(&Graph::new(), &missing).unwrap_err();
        assert!(err.to_string().contains(&missing.display().to_string()));
    }

    #[test]
    fn file_stems() {
        assert_eq!(file_stem("Jane"), "Jane");
        assert_eq!(file_stem("a/b: c?"), "a_b_ c_");
        assert_eq!(file_stem("..."), "untitled");
        assert_eq!(file_stem(".hidden"), "hidden");
        assert_eq!(file_stem("con"), "con_");
    }

    #[test]
    fn namespace_parents() {
        assert_eq!(namespace_parent("a/b/c").as_deref(), Some("a/b"));
        assert_eq!(namespace_parent("a / b").as_deref(), Some("a"));
        assert_eq!(namespace_parent("a"), None);
        assert_eq!(namespace_parent("/a"), None);
        assert_eq!(namespace_parent("a/"), None);
    }

    #[test]
    fn unique_paths() {
        let mut used = HashSet::new();
        let dir = Path::new("x");
        assert_eq!(unique_path(dir, "Jane", &mut used), dir.join("Jane.md"));
        assert_eq!(unique_path(dir, "jane", &mut used), dir.join("jane 2.md"));
        assert_eq!(unique_path(dir, "Jane", &mut used), dir.join("Jane 3.md"));
    }
}
