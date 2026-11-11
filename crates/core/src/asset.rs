//! The `##blob` line: how a binary is described in the graph.
//!
//! A blob is a block whose first line is `##blob(hash, mime, size, name)`; any
//! text after it is the blob's caption. The hash is the BLAKE3 hash of the
//! bytes, in hex, and the bytes themselves are kept outside the graph. The
//! name, the file name, may be left off. A blob is a block, not an entity: it
//! has no page of its own, and sits in whichever page it was added to.
//!
//! The type is called [`Asset`], for what the line describes.

use quiddity_syntax::{format_meta_line, normalise_name, parse_meta_line};

use crate::{BlockId, Graph, Result as GraphResult};

/// The `##` property that makes a block a blob.
pub const KEY: &str = "blob";

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

    /// Reads the parameter of a `##blob` line. The file name is whatever
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

    /// The file this asset is in an export: `<hash>.<extension>`, in `assets/`.
    ///
    /// The extension is the name's, if it has a short plain one, so that a
    /// file manager or a browser knows what it is; otherwise the MIME type's,
    /// if it is a common one; otherwise `bin`. It is always lower-case letters
    /// and digits, so it is safe as part of a path whatever the name says. The
    /// hash is what identifies the file: the extension is only a convenience.
    pub fn file_name(&self) -> String {
        format!("{}.{}", self.hash, self.extension())
    }

    fn extension(&self) -> String {
        let named = self
            .name
            .as_deref()
            .and_then(|n| std::path::Path::new(n).extension())
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .filter(|e| (1..=8).contains(&e.len()) && e.bytes().all(|b| b.is_ascii_alphanumeric()));
        named.unwrap_or_else(|| {
            let known = match self.mime.as_str() {
                "image/png" => "png",
                "image/jpeg" => "jpg",
                "image/gif" => "gif",
                "image/webp" => "webp",
                "image/avif" => "avif",
                "image/svg+xml" => "svg",
                "application/pdf" => "pdf",
                "application/json" => "json",
                "text/plain" => "txt",
                "text/markdown" => "md",
                _ => "bin",
            };
            known.to_owned()
        })
    }

    /// The parameter of the `##blob` line that says this.
    pub fn parameter(&self) -> String {
        let mut p = format!("{}, {}, {}", self.hash, self.mime, self.size);
        if let Some(name) = &self.name {
            p.push_str(", ");
            p.push_str(&escape(name));
        }
        p
    }

    /// The `##blob` line that says this.
    pub fn line(&self) -> String {
        format_meta_line(KEY, Some(&self.parameter()))
    }

    /// Reads the blob a block's text says it is, if its first line is a
    /// `##blob` line: `None` for any other block, an error for a line that
    /// can't be read.
    pub fn in_block(text: &str) -> Option<Result<Self, AssetError>> {
        let parameter = blob_line(text)?;
        Some(
            parameter
                .as_deref()
                .ok_or(AssetError::Missing)
                .and_then(Self::parse),
        )
    }

    /// The parameter of the `##blob` line a block starts with, as written,
    /// whether or not it can be read: the outer `None` is a block that is no
    /// blob, the inner one a line with no parameter.
    pub fn line_of(text: &str) -> Option<Option<String>> {
        blob_line(text)
    }

    /// The caption of the blob in `text`: what follows its first line.
    pub fn caption(text: &str) -> &str {
        text.split_once('\n').map_or("", |(_, rest)| rest).trim()
    }
}

/// The parameter of a first line that is a `##blob` line.
fn blob_line(text: &str) -> Option<Option<String>> {
    let (key, parameter) = parse_meta_line(text.lines().next()?)?;
    (normalise_name(&key) == KEY).then_some(parameter)
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
    /// Adds a blob to the end of `parent`'s children: a block whose first line
    /// says what the file is. It makes no entity, and has no caption until
    /// someone types one.
    pub fn create_blob(&self, parent: BlockId, asset: &Asset) -> GraphResult<BlockId> {
        self.create_block(Some(parent), None, &asset.line())
    }
}
