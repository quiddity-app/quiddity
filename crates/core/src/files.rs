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
use quiddity_syntax::{Value, format_property_line};

use crate::merge::is_spent_duplicate;
use crate::outline::{Indent, export_entity, import_outline, parse_outline};
use crate::{BlockId, Graph, Identity, Index, Result, Settings};

pub const MANIFEST: &str = "quiddity.json";
const MANIFEST_BODY: &str = "{\"format\": \"quiddity\", \"version\": 1}\n";
/// The graph's settings: see [`Settings`].
pub const SETTINGS: &str = "quiddity.toml";

/// Writes every entity to `dir`, which must be empty or not yet exist.
/// Returns the number of files written.
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
        fs::write(path, export_entity(graph, entity)?)?;
        written += 1;
    }
    Ok(written)
}

/// Imports every Markdown file under `dir` as an entity. Files in a
/// `journals` directory named for a date (`2026-10-25.md`, or Logseq's
/// `2026_10_25.md`) become journal pages. A `quiddity.toml` sets the graph's
/// settings; those it doesn't mention are left as they were.
pub fn import_dir(graph: &Graph, dir: &Path) -> Result<Vec<BlockId>> {
    // Read first, so that a mistake in it stops the import before anything
    // has been added.
    let settings = match fs::read_to_string(dir.join(SETTINGS)) {
        Ok(text) => Some(Settings::from_toml(&text)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let indent = if dir.join(MANIFEST).is_file() {
        Indent::Tabs
    } else {
        Indent::Auto
    };
    let mut files = Vec::new();
    collect_markdown(dir, &mut files)?;
    files.sort();
    let mut ids = Vec::new();
    for path in files {
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
            let has_type = outline.properties.as_deref().is_some_and(|p| {
                p.lines().any(|l| {
                    quiddity_syntax::parse_property_line(l)
                        .is_some_and(|(k, _)| k.eq_ignore_ascii_case("type"))
                })
            });
            if !has_type {
                let date = date.to_string();
                let mut lines = vec![
                    format_property_line("type", Some(&Value::Text("journal".into()))),
                    format_property_line("date", Some(&Value::Text(date.clone()))),
                ];
                lines.extend(outline.properties.take());
                outline.properties = Some(lines.join("\n"));
                outline.title.get_or_insert(date);
            }
        }
        ids.push(import_outline(graph, &outline, &stem)?);
    }
    if let Some(settings) = settings {
        graph.apply_settings(&settings)?;
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

    #[test]
    fn file_stems() {
        assert_eq!(file_stem("Jane"), "Jane");
        assert_eq!(file_stem("a/b: c?"), "a_b_ c_");
        assert_eq!(file_stem("..."), "untitled");
        assert_eq!(file_stem(".hidden"), "hidden");
        assert_eq!(file_stem("con"), "con_");
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
