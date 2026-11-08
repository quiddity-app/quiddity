//! The `##asset` line: how a binary is described in the graph.
//!
//! An asset is an entity whose properties block holds
//! `##asset(hash, mime, size, name)`. The hash is the BLAKE3 hash of the
//! bytes, in hex, and the bytes themselves are kept outside the graph. The
//! name, the file name, may be left off.

use quiddity_syntax::{MetaProperty, format_meta_line, normalise_name};

use crate::{BlockId, Graph, Result as GraphResult};

/// The meta-property that says an entity is an asset.
pub const KEY: &str = "asset";

/// What the graph says about a binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    /// The BLAKE3 hash of the bytes, as 64 lower-case hex digits.
    pub hash: String,
    /// The MIME type, lower-case, as `type/subtype`.
    pub mime: String,
    /// The length of the bytes.
    pub size: u64,
    /// The file name, if it is known.
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssetError {
    #[error("needs a parameter: hash, MIME type, size and optionally a file name")]
    Missing,
    #[error("the hash must be 64 hexadecimal digits")]
    Hash,
    #[error("the MIME type must look like type/subtype")]
    Mime,
    #[error("the size must be a whole number of bytes")]
    Size,
    #[error("an entity can describe only one asset")]
    Several,
}

impl Asset {
    /// An asset from its parts, checked as [`Asset::parse`] checks them. The
    /// file name is trimmed, and a line break in it becomes a space, since the
    /// line it is written in has one line.
    pub fn new(hash: &str, mime: &str, size: u64, name: Option<&str>) -> Result<Self, AssetError> {
        Ok(Self {
            hash: check_hash(hash)?,
            mime: check_mime(mime)?,
            size,
            name: name.and_then(clean_name),
        })
    }

    /// Reads the parameter of an `##asset` line. The file name is whatever
    /// follows the third comma, so it may contain commas.
    pub fn parse(parameter: &str) -> Result<Self, AssetError> {
        let mut parts = parameter.splitn(4, ',');
        let mut next = || parts.next().map(str::trim);
        let (hash, mime, size) = (next(), next(), next());
        let hash = check_hash(hash.unwrap_or_default())?;
        let mime = check_mime(mime.unwrap_or_default())?;
        let size = size.and_then(|s| s.parse().ok()).ok_or(AssetError::Size)?;
        let name = parts.next().map(unescape);
        Ok(Self {
            hash,
            mime,
            size,
            name: name.as_deref().and_then(clean_name),
        })
    }

    /// The parameter of the `##asset` line that says this.
    pub fn parameter(&self) -> String {
        let mut p = format!("{}, {}, {}", self.hash, self.mime, self.size);
        if let Some(name) = &self.name {
            p.push_str(", ");
            p.push_str(&escape(name));
        }
        p
    }

    /// The `##asset` line that says this.
    pub fn line(&self) -> String {
        format_meta_line(KEY, Some(&self.parameter()))
    }

    /// Reads an entity's `##asset` line, if it has one.
    pub fn of(meta: &[MetaProperty]) -> Option<Result<Self, AssetError>> {
        let mut lines = meta.iter().filter(|m| is_asset(m));
        let first = lines.next()?;
        if lines.next().is_some() {
            return Some(Err(AssetError::Several));
        }
        Some(
            first
                .parameter
                .as_deref()
                .ok_or(AssetError::Missing)
                .and_then(Self::parse),
        )
    }
}

fn check_hash(hash: &str) -> Result<String, AssetError> {
    if hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(hash.to_ascii_lowercase())
    } else {
        Err(AssetError::Hash)
    }
}

fn check_mime(mime: &str) -> Result<String, AssetError> {
    let mime = mime.to_ascii_lowercase();
    let token = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_graphic() && !matches!(b, b'/' | b'\\' | b'(' | b')'))
    };
    if mime
        .split_once('/')
        .is_some_and(|(t, s)| token(t) && token(s))
    {
        Ok(mime)
    } else {
        Err(AssetError::Mime)
    }
}

/// A file name as the line holds it: on one line, with the space around it
/// gone. None if nothing is left.
fn clean_name(name: &str) -> Option<String> {
    let name: String = name
        .chars()
        .map(|c| if matches!(c, '\n' | '\r') { ' ' } else { c })
        .collect();
    let name = name.trim_matches([' ', '\t']);
    (!name.is_empty()).then(|| name.to_owned())
}

/// Whether a meta-property line is an `##asset`.
pub(crate) fn is_asset(m: &MetaProperty) -> bool {
    normalise_name(&m.key) == KEY
}

/// What a meta-property parameter reads as a backslash escape.
fn escapable(c: char) -> bool {
    matches!(c, '\\' | '(' | ')' | '[' | ']' | ',' | '"' | '#')
}

/// Escapes what would end the name early, or be read as something else.
fn escape(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '\\' | '(' | ')' | '[' | ']' | '"' | '#' => {
                out.push('\\');
                out.push(c);
            }
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn unescape(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        match chars.peek() {
            Some(&n) if c == '\\' && escapable(n) => {
                out.push(n);
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

impl Graph {
    /// Creates an entity for a binary: named for its file, or, with no name,
    /// for the start of its hash, with the `##asset` line in its properties
    /// block.
    pub fn create_asset(&self, asset: &Asset) -> GraphResult<BlockId> {
        let title = asset
            .name
            .clone()
            .unwrap_or_else(|| asset.hash[..12].to_owned());
        let entity = self.create_block(None, None, &title)?;
        self.create_block(Some(entity), None, &asset.line())?;
        Ok(entity)
    }
}
