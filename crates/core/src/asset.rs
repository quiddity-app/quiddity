//! The `##asset` line: how a binary is described in the graph.
//!
//! An asset is an entity whose properties block holds
//! `##asset(hash, mime, size, name)`. The hash is the BLAKE3 hash of the
//! bytes, in hex, and the bytes themselves are kept outside the graph. The
//! name, the file name, may be left off.

use quiddity_syntax::{MetaProperty, format_meta_line, normalise_name};

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
    /// Reads the parameter of an `##asset` line. The file name is whatever
    /// follows the third comma, so it may contain commas.
    pub fn parse(parameter: &str) -> Result<Self, AssetError> {
        let mut parts = parameter.splitn(4, ',');
        let mut next = || parts.next().map(str::trim);
        let (hash, mime, size) = (next(), next(), next());
        let hash = hash.filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()));
        let hash = hash.ok_or(AssetError::Hash)?;
        let mime = mime.map(str::to_ascii_lowercase).unwrap_or_default();
        let token = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_graphic() && !matches!(b, b'/' | b'\\' | b'(' | b')'))
        };
        if !mime
            .split_once('/')
            .is_some_and(|(t, s)| token(t) && token(s))
        {
            return Err(AssetError::Mime);
        }
        let size = size.and_then(|s| s.parse().ok()).ok_or(AssetError::Size)?;
        let name = parts
            .next()
            .map(|n| unescape(n.trim_matches([' ', '\t'])))
            .filter(|n| !n.is_empty());
        Ok(Self {
            hash: hash.to_ascii_lowercase(),
            mime,
            size,
            name,
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
