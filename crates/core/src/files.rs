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

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use jiff::civil::Date;
use quiddity_syntax::{
    Link, PropertyStyle, Value, fold, format_meta_line, format_property_line, normalise_name,
    parse_block, parse_meta_line, parse_property_line,
};
use tracing::{debug, instrument, trace, warn};

use crate::merge::is_spent_duplicate;
use crate::outline::{Indent, Outline, OutlineBlock, export_entity, import_outline, parse_outline};
use crate::{BlockId, Graph, Identity, Index, Result, Settings};

pub const MANIFEST: &str = "quiddity.json";
const MANIFEST_BODY: &str = "{\"format\": \"quiddity\", \"version\": 1}\n";
/// The graph's settings: see [`Settings`].
pub const SETTINGS: &str = "quiddity.toml";

/// Writes every entity to `dir`, which must be empty or not yet exist.
/// Returns the number of files written.
///
/// If writing fails part way, what was written is removed (the directory too,
/// if it was made for this), so that the export can be tried again: it
/// refuses a directory that isn't empty, which a half-written one would be.
#[instrument(level = "debug", skip(graph), fields(dir = %dir.display()))]
pub fn export_dir(graph: &Graph, dir: &Path) -> Result<usize> {
    let existed = dir.exists();
    if existed && fs::read_dir(dir)?.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} is not empty", dir.display()),
        )
        .into());
    }
    // Only Windows limits the whole path. Cutting names for it elsewhere would
    // make an export's file names depend on where it was written.
    removing_on_failure(dir, existed, || write_export(graph, dir, cfg!(windows)))
}

/// Runs `write`, which fills `dir`. If it fails, takes out what it wrote and
/// returns its error: the directory itself unless it `existed` already, and
/// then only what is in it, which the caller found empty.
fn removing_on_failure<T>(
    dir: &Path,
    existed: bool,
    write: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let result = write();
    if result.is_err() {
        let removed = if existed {
            empty_dir(dir)
        } else {
            fs::remove_dir_all(dir)
        };
        if let Err(error) = removed {
            warn!(dir = %dir.display(), %error, "couldn't remove a failed export");
        }
    }
    result
}

/// Removes everything in `dir`, and not `dir`.
fn empty_dir(dir: &Path) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

/// `limit_path` keeps every file's whole path within Windows' limit.
fn write_export(graph: &Graph, dir: &Path, limit_path: bool) -> Result<usize> {
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
    let stem_units = if limit_path {
        windows_stem_budget(dir)
    } else {
        usize::MAX
    };
    // Entities with one name are numbered in the order they are met, so that
    // order must not depend on where they sit among the roots, which moves,
    // or the files of one export swap contents in the next. A block's ID is
    // the same on every device and never changes. Within one device it is
    // creation order, so the oldest `Jane` is `Jane.md`.
    let mut roots = graph.roots();
    roots.sort_unstable();
    for entity in roots {
        if is_spent_duplicate(graph, &index, entity)? {
            continue;
        }
        let info = index.entity(entity).expect("every root is indexed");
        let (sub, stem) = match info.identity() {
            Some(Identity::Journal(date)) => ("journals", date.to_string()),
            _ => ("entities", file_stem_within(&info.name, stem_units)),
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
///
/// A Logseq graph also links to its journals by the date as Logseq writes it
/// (`[[10th November 2026]]`), which isn't the name a journal has here. So a
/// journal that is linked to that way gets the Logseq form as an `alias`, and
/// one that is linked to but has no file (Logseq makes a day's file only once
/// something is written on it) is made, so that the link has somewhere to go.
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
    // Every file is read before any is imported, since a journal needs to know
    // whether a page that comes after it links to it by date.
    let mut parsed = Vec::with_capacity(files.len());
    for path in files {
        let outline = parse_outline(&fs::read_to_string(&path)?, indent);
        parsed.push((path, outline));
    }
    let linked = if logseq {
        linked_days(parsed.iter().map(|(_, outline)| outline))
    } else {
        BTreeSet::new()
    };
    let mut ids = Vec::new();
    let mut ancestors = Vec::new();
    let mut journalled = HashSet::new();
    for (path, mut outline) in parsed {
        trace!(path = %path.display(), "importing");
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
            if !has_journal_line(&outline) {
                let date = date.to_string();
                let mut lines = vec![format_meta_line(crate::journal::KEY, Some(&date))];
                lines.extend(outline.properties.take());
                outline.properties = Some(lines.join("\n"));
                outline.title.get_or_insert(date);
            }
            if linked.contains(&date) {
                append_property(&mut outline, &alias_line(date));
            }
            journalled.insert(date);
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
    for date in linked.iter().filter(|d| !journalled.contains(*d)) {
        let day = date.to_string();
        let outline = Outline {
            title: Some(day.clone()),
            properties: Some(format!(
                "{}\n{}",
                format_meta_line(crate::journal::KEY, Some(&day)),
                alias_line(*date)
            )),
            blocks: Vec::new(),
        };
        ids.push(import_outline(graph, &outline, &day)?);
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

/// Whether the outline's properties block says it is a day's journal.
fn has_journal_line(outline: &Outline) -> bool {
    outline.properties.as_deref().is_some_and(|p| {
        p.lines().any(|l| {
            parse_meta_line(l).is_some_and(|(k, _)| normalise_name(&k) == crate::journal::KEY)
        })
    })
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
    append_property(outline, &line);
}

/// Adds a line to the end of the outline's properties block.
fn append_property(outline: &mut Outline, line: &str) {
    outline.properties = Some(match outline.properties.take() {
        Some(p) => format!("{p}\n{line}"),
        None => line.to_owned(),
    });
}

/// The months, as Logseq's `MMMM` writes them.
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// A day as Logseq writes it in a link by default (`do MMMM yyyy`):
/// `10th November 2026`.
fn logseq_day_name(date: Date) -> String {
    let day = date.day();
    let suffix = match (day % 10, day / 10) {
        (_, 1) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!(
        "{day}{suffix} {} {}",
        MONTHS[date.month() as usize - 1],
        date.year()
    )
}

/// The day a link's name says, if it is written as [`logseq_day_name`] writes
/// it, whatever the case. The name is built again from the day it seems to
/// say and compared, so `1th January 2026` and `31st June 2026` aren't days.
fn logseq_day(name: &str) -> Option<Date> {
    let mut parts = name.split_whitespace();
    let (day, month, year) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let digits = day.len() - day.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let month = MONTHS.iter().position(|m| m.eq_ignore_ascii_case(month))? + 1;
    let date = Date::new(
        year.parse().ok()?,
        i8::try_from(month).ok()?,
        day[..digits].parse().ok()?,
    )
    .ok()?;
    let canonical = logseq_day_name(date);
    canonical
        .eq_ignore_ascii_case(&format!("{day} {} {year}", MONTHS[month - 1]))
        .then_some(date)
}

/// The `alias` line that lets a link in Logseq's form find the day's journal.
fn alias_line(date: Date) -> String {
    format_property_line("alias", Some(&Value::Text(logseq_day_name(date))))
}

/// The days that the outlines link to by Logseq's name for them, in a link or
/// as a tag (`#[[10th November 2026]]`, which is a link too).
fn linked_days<'a>(outlines: impl Iterator<Item = &'a Outline>) -> BTreeSet<Date> {
    fn read(text: &str, days: &mut BTreeSet<Date>) {
        let block = parse_block(text);
        let tags = block
            .properties
            .iter()
            .filter(|p| p.style == PropertyStyle::Inline)
            .map(|p| &p.key);
        days.extend(
            block
                .links
                .iter()
                .map(|l| &l.name)
                .chain(tags)
                .filter_map(|name| logseq_day(name)),
        );
    }
    fn walk(blocks: &[OutlineBlock], days: &mut BTreeSet<Date>) {
        for block in blocks {
            read(&block.text, days);
            walk(&block.children, days);
        }
    }
    let mut days = BTreeSet::new();
    for outline in outlines {
        if let Some(properties) = &outline.properties {
            read(properties, &mut days);
        }
        walk(&outline.blocks, &mut days);
    }
    days
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
            properties: Some("##inverse(children)".into()),
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

/// The most characters of an entity's name that go in a file name.
const MAX_STEM_CHARS: usize = 120;
/// The most bytes. File systems limit a name's length in bytes (255 on Linux
/// and APFS), not characters, and CJK characters take three and emoji four.
/// What is left of 255 holds a number (`Jane 2`) and `.md`.
const MAX_STEM_BYTES: usize = 200;

/// Windows' limit on a whole path, in UTF-16 units, without the terminating
/// NUL. Without opting in to long paths, a longer one can't be written.
const WINDOWS_MAX_PATH: usize = 259;
/// The least a stem is cut to for the sake of a long path. A directory that
/// deep leaves nothing that fits, and a name this short is still a name.
const MIN_STEM_UNITS: usize = 16;

/// How many UTF-16 units of stem keep a file in `dir` within Windows' path
/// limit: what is left after the directory, `\entities\`, a number (` 999`)
/// and `.md`. The directory is made absolute first, since that is the path
/// Windows measures.
fn windows_stem_budget(dir: &Path) -> usize {
    let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_owned());
    let dir_units = dir.to_string_lossy().encode_utf16().count();
    WINDOWS_MAX_PATH
        .saturating_sub(dir_units + r"\entities\".len() + " 999.md".len())
        .max(MIN_STEM_UNITS)
}

/// A file name for an entity, cut to at most `max_units` UTF-16 units. Names
/// needn't be unique or file-system safe; the real name is kept in the file's
/// `title::`.
fn file_stem_within(name: &str, max_units: usize) -> String {
    let mut bytes = 0;
    let mut units = 0;
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_control() || r#"/\:*?"<>|"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(MAX_STEM_CHARS)
        .take_while(|c| {
            bytes += c.len_utf8();
            units += c.len_utf16();
            bytes <= MAX_STEM_BYTES && units <= max_units
        })
        .collect();
    let safe = safe
        .trim()
        .trim_end_matches(['.', ' '])
        .trim_start_matches('.');
    // Windows reads `con.txt` as the device too, so it is the part before the
    // first dot that counts.
    let (base, rest) = safe.split_at(safe.find('.').unwrap_or(safe.len()));
    match safe {
        "" => "untitled".to_owned(),
        _ if is_device_name(base.trim_end()) => format!("{base}_{rest}"),
        s => s.to_owned(),
    }
}

/// Whether Windows treats `name` as a device, whatever follows a dot and
/// whatever the case. The superscript digits are devices too.
fn is_device_name(name: &str) -> bool {
    matches!(
        name.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "CONIN$"
            | "CONOUT$"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "COM\u{b9}"
            | "COM\u{b2}"
            | "COM\u{b3}"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
            | "LPT\u{b9}"
            | "LPT\u{b2}"
            | "LPT\u{b3}"
    )
}

/// `dir/stem.md`, or `dir/stem 2.md` and so on if taken. Compared as names
/// are everywhere (`fold`: Unicode form and case), so that two names a file
/// system may treat as one path, because it ignores case or normalises
/// `é` composed and decomposed (macOS does both), don't overwrite each other.
fn unique_path(dir: &Path, stem: &str, used: &mut HashSet<String>) -> PathBuf {
    let mut n = 1;
    loop {
        let candidate = if n == 1 {
            stem.to_owned()
        } else {
            format!("{stem} {n}")
        };
        let key = format!("{}/{}", dir.display(), fold(&candidate));
        if used.insert(key) {
            return dir.join(format!("{candidate}.md"));
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    mod logseq_days {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// Whatever day it is, Logseq's name for it reads back as that day, in
            /// any case.
            #[test]
            fn a_days_logseq_name_reads_back(days in -800_000i32..2_000_000) {
                let date = Date::new(1970, 1, 1).unwrap() + jiff::Span::new().days(days);
                let name = logseq_day_name(date);
                prop_assert_eq!(logseq_day(&name), Some(date));
                prop_assert_eq!(logseq_day(&name.to_uppercase()), Some(date));
            }
        }
    }

    #[test]
    fn only_a_day_written_as_logseq_writes_it_is_one() {
        let day = |s| logseq_day(s).map(|d| d.to_string());
        assert_eq!(day("10th November 2026").as_deref(), Some("2026-11-10"));
        for (name, expected) in [
            ("1st January 2026", "2026-01-01"),
            ("2nd february 2026", "2026-02-02"),
            ("3rd March 2026", "2026-03-03"),
            ("11th April 2026", "2026-04-11"),
            ("12th May 2026", "2026-05-12"),
            ("13th June 2026", "2026-06-13"),
            ("21st July 2026", "2026-07-21"),
            ("22nd August 2026", "2026-08-22"),
            ("23rd September 2026", "2026-09-23"),
            ("30th October 2026", "2026-10-30"),
            (" 29th  February   2028 ", "2028-02-29"),
        ] {
            assert_eq!(day(name).as_deref(), Some(expected), "{name}");
        }
        for not in [
            "",
            "November 2026",
            "10 November 2026",
            "10th Nov 2026",
            "11st November 2026",
            "1th November 2026",
            "010th November 2026",
            "31st June 2026",
            "29th February 2026",
            "10th November 02026",
            "10th November 2026 extra",
            "2026-11-10",
            "Places/10th November 2026",
        ] {
            assert_eq!(day(not), None, "{not}");
        }
    }

    /// A stem with no path-length limit, which is every other platform's.
    fn file_stem(name: &str) -> String {
        file_stem_within(name, usize::MAX)
    }

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

    /// Windows reads the part before the first dot as the device, so `con.txt`
    /// (which becomes `con.txt.md`) is as bad as `con`.
    #[test]
    fn device_names_are_found_before_the_first_dot() {
        assert_eq!(file_stem("con.txt"), "con_.txt");
        assert_eq!(file_stem("CON.tar.gz"), "CON_.tar.gz");
        assert_eq!(file_stem("Nul.x"), "Nul_.x");
        assert_eq!(file_stem("COM1"), "COM1_");
        assert_eq!(file_stem("lpt\u{b2}.log"), "lpt\u{b2}_.log");
        assert_eq!(file_stem("conin$"), "conin$_");
        // Not devices: only the whole part before the dot counts.
        assert_eq!(file_stem("Conan.txt"), "Conan.txt");
        assert_eq!(file_stem("a.con"), "a.con");
        assert_eq!(file_stem("com10"), "com10");
        assert_eq!(file_stem("con-x.txt"), "con-x.txt");
    }

    /// The names the file system sees are what is limited, so count bytes:
    /// 130 CJK characters are 390.
    #[test]
    fn long_stems_are_cut_by_bytes_on_a_character() {
        let cjk = file_stem(&"日".repeat(130));
        assert_eq!(cjk, "日".repeat(MAX_STEM_BYTES / 3));
        assert_eq!(file_stem(&"a".repeat(300)), "a".repeat(MAX_STEM_CHARS));
        // Four-byte characters, where the byte limit falls between two.
        assert_eq!(
            file_stem(&"🦀".repeat(100)),
            "🦀".repeat(MAX_STEM_BYTES / 4)
        );
        // The cut doesn't leave a trailing space or dot to be trimmed away
        // into a different name.
        assert_eq!(
            file_stem(&format!("{} x", "a".repeat(MAX_STEM_CHARS - 1))),
            "a".repeat(MAX_STEM_CHARS - 1)
        );
    }

    #[test]
    fn stems_are_cut_to_the_units_they_are_given() {
        assert_eq!(file_stem_within("abcdef", 4), "abcd");
        // A crab is two UTF-16 units, so two fit in five and not three.
        assert_eq!(
            file_stem_within(&"\u{1f980}".repeat(10), 5),
            "\u{1f980}".repeat(2)
        );
        assert_eq!(file_stem_within("ab ", 3), "ab");
        assert_eq!(file_stem_within("abc", usize::MAX), "abc");
    }

    /// The budget is what Windows' path limit leaves of `dir\entities\`, a
    /// number and `.md`, and never so little that nothing is left.
    #[test]
    fn the_windows_budget_is_what_the_path_limit_leaves() {
        let overhead = r"\entities\".len() + " 999.md".len();
        let dir = |n: usize| PathBuf::from(format!("/{}", "d".repeat(n - 1)));
        assert_eq!(
            windows_stem_budget(&dir(40)),
            WINDOWS_MAX_PATH - 40 - overhead
        );
        assert_eq!(
            windows_stem_budget(&dir(200)),
            WINDOWS_MAX_PATH - 200 - overhead
        );
        assert_eq!(windows_stem_budget(&dir(240)), MIN_STEM_UNITS);
        assert_eq!(windows_stem_budget(&dir(900)), MIN_STEM_UNITS);
        // A relative directory is measured where it will be.
        let here = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .encode_utf16()
            .count();
        assert_eq!(
            windows_stem_budget(Path::new("x")),
            WINDOWS_MAX_PATH - (here + 1 + 1) - overhead
        );
    }

    /// With the limit on, a deep directory leaves little room for a name, and
    /// the whole path of every file is within what Windows allows. Without it,
    /// the same names are written whole.
    #[test]
    fn a_limited_export_keeps_whole_paths_short() {
        let tmp = tempfile::tempdir().unwrap();
        let graph = Graph::new();
        graph.create_entity(&"a".repeat(100), &[]).unwrap();
        graph.create_entity(&"\u{1f980}".repeat(40), &[]).unwrap();
        graph.commit();

        let deep = tmp.path().join("d".repeat(200));
        let out = deep.join("out");
        write_export(&graph, &out, true).unwrap();
        let mut longest = 0;
        for entry in fs::read_dir(out.join("entities")).unwrap() {
            let path = entry.unwrap().path();
            longest = longest.max(path.to_string_lossy().encode_utf16().count());
        }
        assert!(longest <= WINDOWS_MAX_PATH, "{longest}");
        assert!(
            longest > WINDOWS_MAX_PATH - 8,
            "cut no more than it had to: {longest}"
        );

        let free = tmp.path().join("free");
        write_export(&graph, &free, false).unwrap();
        let names: Vec<_> = fs::read_dir(free.join("entities"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.contains(&format!("{}.md", "a".repeat(100))),
            "{names:?}"
        );
    }

    mod stems {
        use proptest::prelude::*;

        use super::*;

        proptest! {
            /// However dots, spaces and device names are mixed, the part of
            /// the stem before its first dot is never a device.
            #[test]
            fn stems_are_never_devices(
                name in "(con|PRN|aux|NUL|com1|LPT9|x|\\.|\\.|a|[ ]){1,6}",
            ) {
                let stem = file_stem(&name);
                let base = stem.split('.').next().unwrap();
                prop_assert!(!is_device_name(base.trim_end()), "{stem:?}");
            }

            /// Whatever the name, the stem fits with room for a number and
            /// the extension, and is never empty.
            #[test]
            fn stems_fit_in_a_file_name(name in "(.|日|🦀|é|/| |\\.){0,300}") {
                let stem = file_stem(&name);
                prop_assert!(!stem.is_empty());
                prop_assert!(stem.chars().count() <= MAX_STEM_CHARS);
                prop_assert!(format!("{stem} 99999.md").len() <= 255, "{stem:?}");
            }
        }
    }

    /// What a failed write leaves behind: a file and a nested directory.
    fn half_write(dir: &Path) -> Result<()> {
        fs::create_dir_all(dir.join("entities"))?;
        fs::write(dir.join("entities/Jane.md"), "x")?;
        fs::write(dir.join("quiddity.json"), "{}")?;
        Err(io::Error::other("disk full").into())
    }

    #[test]
    fn a_failed_export_takes_out_the_directory_it_made() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("out");
        let err = removing_on_failure(&dir, false, || half_write(&dir)).unwrap_err();
        assert!(err.to_string().contains("disk full"), "{err}");
        assert!(!dir.exists());
    }

    #[test]
    fn a_failed_export_empties_a_directory_that_was_there() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("out");
        fs::create_dir(&dir).unwrap();
        assert!(removing_on_failure(&dir, true, || half_write(&dir)).is_err());
        assert!(dir.is_dir());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    }

    #[test]
    fn a_successful_export_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("out");
        for existed in [false, true] {
            let _ = fs::remove_dir_all(&dir);
            if existed {
                fs::create_dir(&dir).unwrap();
            }
            let n = removing_on_failure(&dir, existed, || {
                let _ = half_write(&dir);
                Ok(7)
            })
            .unwrap();
            assert_eq!(n, 7);
            assert!(dir.join("entities/Jane.md").is_file());
        }
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

    /// `Café` composed and decomposed are one path on macOS, so the second
    /// must not take the first's name.
    #[test]
    fn unique_paths_ignore_unicode_form() {
        let mut used = HashSet::new();
        let dir = Path::new("x");
        assert_eq!(
            unique_path(dir, "Caf\u{e9}", &mut used),
            dir.join("Caf\u{e9}.md")
        );
        assert_eq!(
            unique_path(dir, "Cafe\u{301}", &mut used),
            dir.join("Cafe\u{301} 2.md")
        );
        assert_eq!(
            unique_path(dir, "CAF\u{c9}", &mut used),
            dir.join("CAF\u{c9} 3.md")
        );
    }

    /// Different directories don't compete for names.
    #[test]
    fn unique_paths_are_per_directory() {
        let mut used = HashSet::new();
        let (a, b) = (Path::new("a"), Path::new("b"));
        assert_eq!(unique_path(a, "Jane", &mut used), a.join("Jane.md"));
        assert_eq!(unique_path(b, "Jane", &mut used), b.join("Jane.md"));
    }

    mod props {
        use proptest::prelude::*;

        use super::*;

        /// Stems weighted towards what differs only in form or case.
        fn stem() -> impl Strategy<Value = String> {
            prop::collection::vec(
                prop::sample::select(vec![
                    "a", "A", "b", "e", "E", "\u{e9}", "\u{c9}", "\u{301}", "\u{df}", "ss", " ",
                    "2",
                ]),
                1..5,
            )
            .prop_map(|parts| parts.concat())
        }

        proptest! {
            /// However the stems are spelt, no two get the same path once
            /// case and Unicode form are ignored, and a stem nobody has taken
            /// keeps its name.
            #[test]
            fn paths_never_collide_by_form_or_case(stems in prop::collection::vec(stem(), 1..8)) {
                let mut used = HashSet::new();
                let dir = Path::new("x");
                let mut seen = HashSet::new();
                for stem in &stems {
                    let free = !seen.contains(&fold(stem));
                    let path = unique_path(dir, stem, &mut used);
                    let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
                    prop_assert!(seen.insert(fold(&name)), "{name:?} was already taken");
                    if free {
                        prop_assert_eq!(path, dir.join(format!("{stem}.md")));
                    }
                }
            }
        }
    }
}
