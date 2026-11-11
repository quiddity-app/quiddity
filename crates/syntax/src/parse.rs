use std::ops::Range;

use crate::query::query_escapable;

/// A property value as written in Markdown.
///
/// Values are untyped at this level: a property's schema decides whether
/// `2026-10-25` is a date or just text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// Plain or quoted text, with escapes removed.
    Text(String),
    /// A link to an entity, `[[Name#prop(value)]]`.
    Link(Link),
    /// A reference to a block by its `id` property, `((id))`.
    BlockRef(String),
    /// An explicit list, `[a, b]`. Lists don't nest.
    List(Vec<Value>),
}

/// A link to an entity by name, optionally narrowed by the target's properties.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub name: String,
    pub qualifiers: Vec<Qualifier>,
}

/// One `#key` or `#key(value)` inside a link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Qualifier {
    pub key: String,
    pub value: Option<Value>,
}

/// How a property was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyStyle {
    /// `key:: value` on a line of its own.
    Line,
    /// `#key` within the text: a tag. It has no value, since a `(value)`
    /// after it is ordinary text.
    Inline,
}

/// One occurrence of a property in a block's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub key: String,
    pub value: Option<Value>,
    pub style: PropertyStyle,
    /// Byte range of the occurrence within the block's text.
    pub span: Range<usize>,
}

/// A `##key`, `##key(parameter)` or `##key:: value` line: a built-in
/// property, which Quiddity gives a meaning of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaProperty {
    pub key: String,
    /// What follows `::`, for a `##key:: value` line. Such a line has no
    /// parameter, and a line with a parameter has no value.
    pub value: Option<Value>,
    /// What is between the parentheses, trimmed and as written: escapes
    /// are left in, for whatever reads it (often [`crate::parse_query`]).
    pub parameter: Option<String>,
    /// Byte range of the whole line within the block's text.
    pub span: Range<usize>,
}

/// Everything Quiddity cares about in a block's text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Block {
    pub properties: Vec<Property>,
    /// Meta-property lines, in order.
    pub meta: Vec<MetaProperty>,
    /// Links in the text and in property values, in order of appearance.
    pub links: Vec<Link>,
    /// Block references in the text, in order of appearance.
    pub block_refs: Vec<String>,
}

/// Parses a block's text. Never fails: malformed syntax is plain text.
pub fn parse_block(text: &str) -> Block {
    let mut block = Block::default();
    let mut fence: Option<(char, usize)> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let content = content.strip_suffix('\r').unwrap_or(content);
        match (fence_marker(content), fence) {
            (Some((c, n, _)), None) => fence = Some((c, n)),
            (Some((c, n, true)), Some((open, len))) if c == open && n >= len => fence = None,
            (_, Some(_)) => {}
            (None, None) => parse_line(content, offset, &mut block),
        }
        offset += line.len();
    }
    block
}

/// An entity's properties block: only its property lines, tags and `##` lines
/// are read, and the rest is said to be stray.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropertiesBlock {
    /// What the block's property, tag and `##` lines say. Spans are within
    /// the block's text.
    pub block: Block,
    /// Byte ranges of the lines that are none of those: prose, which a
    /// properties block can't hold. They are read as nothing.
    pub stray: Vec<Range<usize>>,
}

/// Reads `text` as a properties block, or says it isn't one: a block is one
/// when its first line that isn't blank is a `key:: value` line, a line of
/// nothing but tags (`#a #b`) or a `##` line, except a `##blob` line, which
/// makes a block a blob and nothing else. Any later line that is none of
/// those is stray, so prose typed into a properties block neither becomes a
/// property nor hides the others.
pub fn parse_properties_block(text: &str) -> Option<PropertiesBlock> {
    let mut found = PropertiesBlock::default();
    let mut offset = 0;
    let mut started = false;
    for line in text.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let content = content.strip_suffix('\r').unwrap_or(content);
        if !content.trim().is_empty() {
            if !started && starts_a_blob(content) {
                return None;
            }
            if is_properties_line(content) {
                parse_line(content, offset, &mut found.block);
            } else if started {
                found.stray.push(offset..offset + content.len());
            } else {
                return None;
            }
            started = true;
        }
        offset += line.len();
    }
    started.then_some(found)
}

/// Whether a line is a `##blob` line, in either form, which is what makes its
/// block a blob.
fn starts_a_blob(line: &str) -> bool {
    let key = parse_meta_line(line)
        .map(|(key, _)| key)
        .or_else(|| parse_meta_value_line(line).map(|(key, _)| key));
    key.is_some_and(|key| crate::normalise_name(&key) == "blob")
}

/// Whether a line belongs in a properties block.
fn is_properties_line(line: &str) -> bool {
    parse_property_line(line).is_some()
        || parse_meta_line(line).is_some()
        || parse_meta_value_line(line).is_some()
        || is_tags_line(line)
}

/// Whether a line is nothing but tags, `#a #[[b c]]`.
fn is_tags_line(line: &str) -> bool {
    let mut sc = Scanner::new(line);
    sc.skip_ws();
    if sc.at_end() {
        return false;
    }
    while !sc.at_end() {
        if !sc.eat("#") || parse_key(&mut sc).is_none() {
            return false;
        }
        sc.skip_ws();
    }
    true
}

/// Whether a block consists only of `key:: value` lines (and `##` lines), like the initial block of an entity that holds the entity's
/// properties.
pub fn is_properties_block(text: &str) -> bool {
    let mut any = false;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        if parse_property_line(line).is_none()
            && parse_meta_line(line).is_none()
            && parse_meta_value_line(line).is_none()
        {
            return false;
        }
        any = true;
    }
    any
}

/// Parses a `key:: value` line, or a line that is only `#key`, which is the
/// same property without a value.
///
/// The key must start with a letter, digit or underscore and may not contain
/// `:`, `#`, brackets or parentheses. `::` must be followed by whitespace or
/// the end of the line.
pub fn parse_property_line(line: &str) -> Option<(String, Option<Value>)> {
    split_property_line(line)
        .map(|(key, value)| (key, value.map(parse_line_value)))
        .or_else(|| hash_property_line(line).map(|key| (key, None)))
}

/// Parses a line that is nothing but `##key` or `##key(parameter)`: a
/// meta-property. The parameter is kept as written, trimmed of spaces and
/// tabs, and `()` is no parameter.
///
/// Parentheses in the parameter must balance, except where escaped, inside a
/// link, or inside a quoted name at the start of a query term, which is where
/// [`crate::parse_query`] reads quotes. So a query needs no escaping.
pub fn parse_meta_line(line: &str) -> Option<(String, Option<String>)> {
    let mut sc = Scanner::new(line.trim().strip_prefix("##")?);
    let key = parse_key(&mut sc)?;
    if sc.at_end() {
        return Some((key, None));
    }
    if !sc.eat("(") {
        return None;
    }
    let start = sc.pos;
    let mut depth = 0usize;
    // The last character that wasn't whitespace, to tell where a term starts.
    let mut prev: Option<char> = Some('(');
    loop {
        let c = sc.peek()?;
        // Escapes, keys, quotes and links are skipped as the query parser
        // reads them, so that the two agree on where the parameter ends.
        if c == '\\' && sc.rest()[1..].chars().next().is_some_and(query_escapable) {
            sc.bump();
            sc.bump();
            // An escaped operator is part of a name, so no term starts after it.
            prev = Some('\\');
            continue;
        }
        match c {
            ')' if depth == 0 => break,
            '#' => {
                sc.bump();
                parse_key(&mut sc);
            }
            '"' if prev.is_some_and(|p| matches!(p, '(' | '&' | '|' | '~')) => {
                parse_quoted(&mut sc)?;
            }
            '[' => {
                if parse_link(&mut sc).is_none() {
                    sc.bump();
                }
            }
            _ => {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                sc.bump();
            }
        }
        // Only what the query parser skips between terms is whitespace here.
        if !matches!(c, ' ' | '\t') {
            prev = sc.src[..sc.pos].chars().next_back();
        }
    }
    // Only space and tab, the whitespace the scan above allows between terms:
    // trimming a form feed would leave a quote at the start that the scan
    // had read as part of a name, and the parameter would not read back.
    let parameter = sc.src[start..sc.pos].trim_matches([' ', '\t']);
    sc.bump();
    sc.at_end()
        .then(|| (key, (!parameter.is_empty()).then(|| parameter.to_owned())))
}

/// Parses a line that is `##key:: value`: a `##` property whose value is
/// written as a property line's is, so a list or a link needs no query syntax.
/// `::` must be followed by whitespace or the end of the line, and a line with
/// no value is the same as `##key`.
pub fn parse_meta_value_line(line: &str) -> Option<(String, Option<Value>)> {
    split_meta_value_line(line).map(|(key, value)| (key, value.map(parse_line_value)))
}

/// Splits a `##key:: value` line into its key and unparsed value.
fn split_meta_value_line(line: &str) -> Option<(String, Option<&str>)> {
    let mut sc = Scanner::new(line.trim().strip_prefix("##")?);
    let key = parse_key(&mut sc)?;
    let rest = sc.rest().strip_prefix("::")?;
    if !(rest.is_empty() || rest.starts_with([' ', '\t'])) {
        return None;
    }
    let rest = rest.trim();
    Some((key, (!rest.is_empty()).then_some(rest)))
}

/// The key of a line that is nothing but `#key`.
fn hash_property_line(line: &str) -> Option<String> {
    let mut sc = Scanner::new(line.trim().strip_prefix('#')?);
    let key = parse_key(&mut sc)?;
    sc.at_end().then_some(key)
}

/// Splits a `key:: value` line into its key and unparsed value.
fn split_property_line(line: &str) -> Option<(String, Option<&str>)> {
    let trimmed = line.trim_start();
    let sep = trimmed.find("::")?;
    let (key, rest) = (&trimmed[..sep], &trimmed[sep + 2..]);
    let key_ok = key.chars().next().is_some_and(is_key_start)
        && !key.ends_with(char::is_whitespace)
        && !key.contains([':', '#', '[', ']', '(', ')', '`']);
    if !key_ok || !(rest.is_empty() || rest.starts_with([' ', '\t'])) {
        return None;
    }
    let rest = rest.trim();
    Some((key.to_owned(), (!rest.is_empty()).then_some(rest)))
}

/// Parses the value part of a `key:: value` line.
///
/// A value that is exactly a list, quoted string, link or block reference is
/// parsed as such; anything else is the text as written.
pub fn parse_line_value(value: &str) -> Value {
    line_value(value).0
}

/// A line value with the links and block references in it. As in inline
/// values, those inside quoted text don't count.
fn line_value(value: &str) -> (Value, Refs) {
    let mut sc = Scanner::new(value);
    let mut refs = Refs::default();
    let parsed = if sc.at_list_start() {
        parse_list(&mut sc, &mut refs)
    } else {
        parse_scalar(&mut sc, &[], &mut refs)
    };
    sc.skip_ws();
    match parsed {
        Some(Some(v)) if sc.at_end() => (v, refs),
        _ => {
            let mut scanned = Block::default();
            scan_inline(value, 0, false, &mut scanned);
            let refs = Refs {
                links: scanned.links,
                block_refs: scanned.block_refs,
            };
            (Value::Text(value.to_owned()), refs)
        }
    }
}

/// Links and block references found while parsing a value.
#[derive(Default)]
struct Refs {
    links: Vec<Link>,
    block_refs: Vec<String>,
}

impl Refs {
    fn add_to(self, block: &mut Block) {
        block.links.extend(self.links);
        block.block_refs.extend(self.block_refs);
    }
}

/// Whether `key` can be written after `#` without `[[…]]` around it.
pub(crate) fn is_bare_key(key: &str) -> bool {
    let mut sc = Scanner::new(key);
    !key.starts_with('[') && parse_key(&mut sc).as_deref() == Some(key) && sc.at_end()
}

fn parse_line(line: &str, offset: usize, block: &mut Block) {
    if let Some((key, parameter)) = parse_meta_line(line) {
        // A parameter that reads as a query links to what it names.
        if let Some(Ok(query)) = parameter.as_deref().map(crate::parse_query) {
            block.links.extend(query.links().into_iter().cloned());
        }
        block.meta.push(MetaProperty {
            key,
            value: None,
            parameter,
            span: offset..offset + line.len(),
        });
    } else if let Some((key, value)) = split_meta_value_line(line) {
        let value = value.map(|v| {
            let (value, refs) = line_value(v);
            refs.add_to(block);
            value
        });
        block.meta.push(MetaProperty {
            key,
            value,
            parameter: None,
            span: offset..offset + line.len(),
        });
    } else if let Some((key, value)) = split_property_line(line) {
        let value = value.map(|v| {
            let (value, refs) = line_value(v);
            refs.add_to(block);
            value
        });
        block.properties.push(Property {
            key,
            value,
            style: PropertyStyle::Line,
            span: offset..offset + line.len(),
        });
    } else {
        scan_inline(line, offset, true, block);
    }
}

fn scan_inline(line: &str, offset: usize, tags: bool, block: &mut Block) {
    let mut sc = Scanner::new(line);
    let mut prev: Option<char> = None;
    while let Some(c) = sc.peek() {
        let at = sc.pos;
        if sc.eat_escape().is_some() {
            prev = Some(c);
            continue;
        }
        match c {
            '`' => skip_code_span(&mut sc),
            '[' if sc.rest().starts_with("[[") => match parse_link(&mut sc) {
                Some(link) => block.links.push(link),
                None => sc.pos += 2,
            },
            '(' if sc.rest().starts_with("((") => match parse_block_ref(&mut sc) {
                Some(id) => block.block_refs.push(id),
                // One step, not two: in `[text](((id)))` the first parenthesis
                // is the link's, and the reference starts at the second.
                None => {
                    sc.bump();
                }
            },
            '#' if tags
                && (prev.is_none_or(|p| p.is_whitespace() || p == '(')
                    || follows_property(block, offset + at)) =>
            {
                sc.pos += 1;
                if let Some(key) = parse_key(&mut sc) {
                    block.properties.push(Property {
                        key,
                        value: None,
                        style: PropertyStyle::Inline,
                        span: offset + at..offset + sc.pos,
                    });
                }
            }
            _ => {
                sc.bump();
            }
        }
        prev = line[..sc.pos].chars().next_back();
    }
}

/// Whether an inline property ends exactly at `pos`, so that `#a(1)#b(2)`
/// is two properties.
fn follows_property(block: &Block, pos: usize) -> bool {
    block
        .properties
        .last()
        .is_some_and(|p| p.style == PropertyStyle::Inline && p.span.end == pos)
}

/// Returns the fence character, its run length, and whether the rest of the
/// line is blank (so it could close a fence).
fn fence_marker(line: &str) -> Option<(char, usize, bool)> {
    let t = line.trim_start();
    let c = t.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let n = t.len() - t.trim_start_matches(c).len();
    (n >= 3).then(|| (c, n, t[n..].trim().is_empty()))
}

fn skip_code_span(sc: &mut Scanner) {
    let rest = sc.rest();
    let run = rest.len() - rest.trim_start_matches('`').len();
    let (fence, after) = rest.split_at(run);
    let mut search = 0;
    while let Some(i) = after[search..].find(fence) {
        let at = search + i;
        let len = after[at..].len() - after[at..].trim_start_matches('`').len();
        if len == run {
            sc.pos += run + at + run;
            return;
        }
        search = at + len;
    }
    sc.pos += run;
}

pub(crate) fn is_key_start(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_key_char(c: char) -> bool {
    crate::name::is_word_char(c) || matches!(c, '_' | '-' | '/' | '.')
}

pub(crate) fn is_escapable(c: char) -> bool {
    matches!(c, '\\' | '(' | ')' | '[' | ']' | ',' | '"' | '#')
}

/// Parses a property key after `#`: either bare (`status`, `due-date`,
/// `area/work`) or bracketed (`[[due date]]`). Trailing full stops belong to
/// the sentence, not the key, and all-digit keys (`#123`) aren't keys.
pub(crate) fn parse_key(sc: &mut Scanner) -> Option<String> {
    let start = sc.pos;
    if let Some(inner) = sc.rest().strip_prefix("[[") {
        let end = inner.find("]]")?;
        let key = inner[..end].trim();
        if key.is_empty() || key.contains(['[', ']']) {
            return None;
        }
        sc.pos += 2 + end + 2;
        return Some(key.to_owned());
    }
    if !sc.peek().is_some_and(is_key_start) {
        return None;
    }
    while sc.peek().is_some_and(is_key_char) {
        sc.bump();
    }
    let key = sc.src[start..sc.pos].trim_end_matches('.');
    sc.pos = start + key.len();
    if key.chars().all(|c| c.is_ascii_digit()) {
        sc.pos = start;
        return None;
    }
    Some(key.to_owned())
}

/// Parses `(value)`. The outer `None` means it wasn't well-formed (and the
/// scanner hasn't moved); the inner `None` means `()`.
fn parse_paren_value(sc: &mut Scanner, refs: &mut Refs) -> Option<Option<Value>> {
    let start = sc.pos;
    let result = parse_paren_value_inner(sc, refs);
    if result.is_none() {
        sc.pos = start;
    }
    result
}

fn parse_paren_value_inner(sc: &mut Scanner, refs: &mut Refs) -> Option<Option<Value>> {
    if !sc.eat("(") {
        return None;
    }
    sc.skip_ws();
    let value = if sc.at_list_start() {
        parse_list(sc, refs)?
    } else {
        parse_scalar(sc, &[')'], refs)?
    };
    sc.skip_ws();
    sc.eat(")").then_some(value)
}

fn parse_list(sc: &mut Scanner, refs: &mut Refs) -> Option<Option<Value>> {
    sc.eat("[");
    let mut items = Vec::new();
    sc.skip_ws();
    if sc.eat("]") {
        return Some(Some(Value::List(items)));
    }
    loop {
        let item = parse_scalar(sc, &[',', ']'], refs)?;
        items.push(item.unwrap_or_else(|| Value::Text(String::new())));
        sc.skip_ws();
        if sc.eat("]") {
            return Some(Some(Value::List(items)));
        }
        if !sc.eat(",") {
            return None;
        }
    }
}

/// Parses a single value up to one of `terms` (or the end, if `terms` is
/// empty). Brackets and parentheses inside it must balance.
fn parse_scalar(sc: &mut Scanner, terms: &[char], refs: &mut Refs) -> Option<Option<Value>> {
    sc.skip_ws();
    if sc.peek() == Some('"') {
        return parse_quoted(sc).map(|s| Some(Value::Text(s)));
    }
    let start = sc.pos;
    let mut text = String::new();
    let mut depth = 0usize;
    loop {
        let Some(c) = sc.peek() else {
            if terms.is_empty() {
                break;
            }
            return None;
        };
        if depth == 0 && terms.contains(&c) {
            break;
        }
        if let Some(e) = sc.eat_escape() {
            text.push(e);
            continue;
        }
        let at = sc.pos;
        if sc.rest().starts_with("[[") {
            if let Some(link) = parse_link(sc) {
                text.push_str(&sc.src[at..sc.pos]);
                refs.links.push(link);
                continue;
            }
        } else if sc.rest().starts_with("((")
            && let Some(id) = parse_block_ref(sc)
        {
            text.push_str(&sc.src[at..sc.pos]);
            refs.block_refs.push(id);
            continue;
        }
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
        text.push(c);
        sc.bump();
    }
    let raw = sc.src[start..sc.pos].trim();
    if raw.is_empty() {
        return Some(None);
    }
    let mut whole = Scanner::new(raw);
    if raw.starts_with("[[")
        && let Some(link) = parse_link(&mut whole).filter(|_| whole.at_end())
    {
        return Some(Some(Value::Link(link)));
    }
    if raw.starts_with("((")
        && let Some(id) = parse_block_ref(&mut whole).filter(|_| whole.at_end())
    {
        return Some(Some(Value::BlockRef(id)));
    }
    Some(Some(Value::Text(text.trim().to_owned())))
}

pub(crate) fn parse_quoted(sc: &mut Scanner) -> Option<String> {
    let start = sc.pos;
    sc.bump();
    let mut s = String::new();
    loop {
        match sc.bump() {
            Some('"') => return Some(s),
            Some('\\') => match sc.bump() {
                Some('n') => s.push('\n'),
                Some('r') => s.push('\r'),
                Some(c) => s.push(c),
                None => break,
            },
            Some(c) => s.push(c),
            None => break,
        }
    }
    sc.pos = start;
    None
}

fn parse_block_ref(sc: &mut Scanner) -> Option<String> {
    let inner = sc.rest().strip_prefix("((")?;
    let end = inner.find("))")?;
    let id = inner[..end].trim();
    if id.is_empty() || id.contains(|c: char| c.is_whitespace() || c == '(' || c == ')') {
        return None;
    }
    sc.pos += 2 + end + 2;
    Some(id.to_owned())
}

/// Parses `[[…]]`. If the qualifiers aren't well-formed, everything up to the
/// first `]]` is taken as the name.
pub(crate) fn parse_link(sc: &mut Scanner) -> Option<Link> {
    let start = sc.pos;
    if let Some(link) = parse_structured_link(sc) {
        return Some(link);
    }
    sc.pos = start;
    let inner = sc.rest().strip_prefix("[[")?;
    let end = inner.find("]]")?;
    let name = inner[..end].trim();
    if name.is_empty() {
        return None;
    }
    sc.pos += 2 + end + 2;
    Some(Link {
        name: name.to_owned(),
        qualifiers: Vec::new(),
    })
}

fn parse_structured_link(sc: &mut Scanner) -> Option<Link> {
    if !sc.eat("[[") {
        return None;
    }
    let mut name = String::new();
    while !sc.rest().starts_with("]]") && !sc.at_qualifier() {
        if let Some(e) = sc.eat_escape() {
            name.push(e);
        } else if sc.rest().starts_with("[[") {
            return None;
        } else {
            name.push(sc.bump()?);
        }
    }
    let mut qualifiers = Vec::new();
    while sc.eat("#") {
        let key = parse_key(sc)?;
        let mut ignored = Refs::default();
        let value = match sc.peek() {
            Some('(') => parse_paren_value(sc, &mut ignored)?,
            _ => None,
        };
        qualifiers.push(Qualifier { key, value });
    }
    let name = name.trim();
    (sc.eat("]]") && !name.is_empty()).then(|| Link {
        name: name.to_owned(),
        qualifiers,
    })
}

pub(crate) struct Scanner<'a> {
    pub(crate) src: &'a str,
    pub(crate) pos: usize,
}

impl<'a> Scanner<'a> {
    pub(crate) fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    pub(crate) fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    pub(crate) fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    pub(crate) fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    pub(crate) fn eat(&mut self, s: &str) -> bool {
        let matched = self.rest().starts_with(s);
        if matched {
            self.pos += s.len();
        }
        matched
    }

    pub(crate) fn eat_escape(&mut self) -> Option<char> {
        let mut chars = self.rest().chars();
        if chars.next()? != '\\' {
            return None;
        }
        let c = chars.next().filter(|&c| is_escapable(c))?;
        self.pos += 1 + c.len_utf8();
        Some(c)
    }

    pub(crate) fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.pos += 1;
        }
    }

    pub(crate) fn at_end(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn at_list_start(&self) -> bool {
        self.rest().starts_with('[') && !self.rest().starts_with("[[")
    }

    fn at_qualifier(&self) -> bool {
        self.rest()
            .strip_prefix('#')
            .is_some_and(|r| r.starts_with("[[") || r.chars().next().is_some_and(is_key_start))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Value {
        Value::Text(s.to_owned())
    }

    fn link(name: &str) -> Link {
        Link {
            name: name.to_owned(),
            qualifiers: Vec::new(),
        }
    }

    fn props(src: &str) -> Vec<(String, Option<Value>)> {
        parse_block(src)
            .properties
            .into_iter()
            .map(|p| (p.key, p.value))
            .collect()
    }

    fn prop(key: &str, value: Option<Value>) -> (String, Option<Value>) {
        (key.to_owned(), value)
    }

    #[test]
    fn property_lines() {
        assert_eq!(
            props("type:: person\nborn:: 1970-01-01\nDue date:: tomorrow"),
            vec![
                prop("type", Some(text("person"))),
                prop("born", Some(text("1970-01-01"))),
                prop("Due date", Some(text("tomorrow"))),
            ]
        );
        assert_eq!(props("empty::"), vec![prop("empty", None)]);
        let lines: Vec<_> = "#flag\n  #[[Due date]]\nkey:: v"
            .lines()
            .map(|l| parse_property_line(l).unwrap())
            .collect();
        assert_eq!(
            lines,
            vec![
                prop("flag", None),
                prop("Due date", None),
                prop("key", Some(text("v")))
            ]
        );
        assert_eq!(
            props("  indented:: yes"),
            vec![prop("indented", Some(text("yes")))]
        );
    }

    #[test]
    fn not_property_lines() {
        for line in [
            "http://example.com::x",
            "no space::after",
            "- bullet:: no",
            "#tag:: no",
            "key :: no",
            ":: no",
            // Prose with a code span that holds `::` (issue 91).
            "An entity with `favourite:: true` is listed on the left.",
            "`code` then key:: value",
        ] {
            assert!(parse_property_line(line).is_none(), "{line}");
        }
        assert!(props("An entity with `favourite:: true` is listed.").is_empty());
        assert!(!is_properties_block(
            "An entity with `favourite:: true` is listed."
        ));
    }

    #[test]
    fn line_values() {
        assert_eq!(
            parse_line_value("[a, b, [[C]]]"),
            Value::List(vec![text("a"), text("b"), Value::Link(link("C"))])
        );
        assert_eq!(parse_line_value("[[Jane]]"), Value::Link(link("Jane")));
        assert_eq!(parse_line_value("((abc))"), Value::BlockRef("abc".into()));
        assert_eq!(parse_line_value("\"quoted, text\""), text("quoted, text"));
        assert_eq!(parse_line_value("a, b"), text("a, b"));
        assert_eq!(
            parse_line_value("met [[Jane]] at lunch"),
            text("met [[Jane]] at lunch")
        );
        assert_eq!(parse_line_value(r"C:\Users\me"), text(r"C:\Users\me"));
        assert_eq!(parse_line_value("\"unterminated"), text("\"unterminated"));
        assert_eq!(parse_line_value("[unterminated"), text("[unterminated"));
        assert_eq!(parse_line_value("[a] and more"), text("[a] and more"));
        assert_eq!(parse_line_value("\"a\" b"), text("\"a\" b"));
    }

    #[test]
    fn inline_properties() {
        assert_eq!(
            props("Call #status about #project, see #[[due date]]."),
            vec![
                prop("status", None),
                prop("project", None),
                prop("due date", None),
            ]
        );
    }

    #[test]
    fn tag_keys_include_combining_marks() {
        assert_eq!(props("#cafe\u{301}"), vec![prop("cafe\u{301}", None)]);
        assert_eq!(props("#caf\u{e9}"), vec![prop("caf\u{e9}", None)]);
    }

    #[test]
    fn a_tag_has_no_value() {
        // What follows a tag is text, whatever it looks like.
        assert_eq!(props("#p(a)"), vec![prop("p", None)]);
        assert_eq!(props("#p([a, b])"), vec![prop("p", None)]);
        assert_eq!(props("#p(a)#p(b)"), vec![prop("p", None)]);
        assert_eq!(props("#a(#b)"), vec![prop("a", None), prop("b", None)]);
        assert_eq!(props("#p()"), vec![prop("p", None)]);
        let block = parse_block("#p([[Jane]]) and #q(((abc)))");
        assert_eq!(
            block.links,
            vec![link("Jane")],
            "a link in the text is a link"
        );
        assert_eq!(block.block_refs, vec!["abc".to_owned()]);
    }

    #[test]
    fn tags_may_abut() {
        assert_eq!(props("#a#b"), vec![prop("a", None), prop("b", None)]);
        assert_eq!(props("#a x#b"), vec![prop("a", None)], "not after a word");
    }

    #[test]
    fn inline_property_spans() {
        let src = "go #to(there) and #[[far away]]";
        let block = parse_block(src);
        let spans: Vec<_> = block
            .properties
            .iter()
            .map(|p| &src[p.span.clone()])
            .collect();
        assert_eq!(spans, ["#to", "#[[far away]]"]);
    }

    #[test]
    fn not_tags() {
        for src in [
            "# Heading",
            "## Heading",
            "issue #123",
            "foo#bar",
            "https://example.com/#frag",
            "`#code`",
            "``a `#b` c``",
            r"\#escaped",
            "#[[ ]]",
            "#[[a[b]]",
            "``a #b`` c",
            "`a`` #b` c",
            "```\n#fenced\n```",
            "~~~~\n#fenced\n~~~\n#still\n~~~~",
        ] {
            assert_eq!(props(src), vec![], "{src}");
        }
    }

    #[test]
    fn tags_after_code_and_fences() {
        assert_eq!(props("`code` #tag"), vec![prop("tag", None)]);
        assert_eq!(props("```\nx\n```\n#tag"), vec![prop("tag", None)]);
        assert_eq!(props("(#tag)"), vec![prop("tag", None)]);
        assert_eq!(props("unclosed `` #tag"), vec![prop("tag", None)]);
        assert_eq!(props("not a link [[ #tag"), vec![prop("tag", None)]);
        assert_eq!(props("not a ref (( #tag"), vec![prop("tag", None)]);
        assert_eq!(props("#tag."), vec![prop("tag", None)]);
        assert_eq!(props("#v1.2"), vec![prop("v1.2", None)]);
        assert_eq!(props("#area/work"), vec![prop("area/work", None)]);
    }

    #[test]
    fn links() {
        let block =
            parse_block("See [[Jane]] and [[Jane#type(person)#born(1970-01-01)]] or [[C# notes]].");
        assert_eq!(
            block.links,
            vec![
                link("Jane"),
                Link {
                    name: "Jane".into(),
                    qualifiers: vec![
                        Qualifier {
                            key: "type".into(),
                            value: Some(text("person"))
                        },
                        Qualifier {
                            key: "born".into(),
                            value: Some(text("1970-01-01"))
                        },
                    ],
                },
                link("C# notes"),
            ]
        );
    }

    #[test]
    fn link_edge_cases() {
        let links = |s: &str| parse_block(s).links;
        assert_eq!(links("[[]]"), vec![]);
        assert_eq!(links("[[unclosed"), vec![]);
        assert_eq!(links("[[C#]]"), vec![link("C#")]);
        assert_eq!(links(r"[[F\#sharp]]"), vec![link("F#sharp")]);
        assert_eq!(links("[[Name#bad(]]"), vec![link("Name#bad(")]);
        let block = parse_block("[[x#y( #k]]");
        assert_eq!(
            (block.links, block.properties),
            (vec![link("x#y( #k")], vec![]),
            "nothing inside a link is a tag"
        );
        assert_eq!(
            links("[[Book#author([[Jane]])]]"),
            vec![Link {
                name: "Book".into(),
                qualifiers: vec![Qualifier {
                    key: "author".into(),
                    value: Some(Value::Link(link("Jane")))
                }],
            }]
        );
        assert_eq!(
            links("[[X#[[multi word]]]]"),
            vec![Link {
                name: "X".into(),
                qualifiers: vec![Qualifier {
                    key: "multi word".into(),
                    value: None
                }]
            }]
        );
    }

    #[test]
    fn references_in_property_values() {
        let block = parse_block(
            "author:: [[Jane]]\nsee:: ((a)) and [[Ann]]\nquoted:: \"[[No]] ((no))\"\n##tag:: [[Bob]]",
        );
        assert_eq!(block.links, vec![link("Jane"), link("Ann"), link("Bob")]);
        assert_eq!(block.block_refs, vec!["a".to_owned()]);
    }

    #[test]
    fn markdown_links_to_entities_and_blocks_count_as_references() {
        let block = parse_block("[the graph]([[Graph view]]) and [a point](((abc-1))) here");
        assert_eq!(
            block
                .links
                .iter()
                .map(|l| l.name.as_str())
                .collect::<Vec<_>>(),
            ["Graph view"]
        );
        assert_eq!(block.block_refs, vec!["abc-1".to_owned()]);
        assert!(parse_block("[x](`((abc))`)").block_refs.is_empty());
    }

    #[test]
    fn block_refs() {
        let block =
            parse_block("As ((0198f0c4-aaaa)) said, (( not a ref )), `((code))`, [((b))], ((c(d))");
        assert_eq!(
            block.block_refs,
            vec!["0198f0c4-aaaa".to_owned(), "b".to_owned()]
        );
    }

    #[test]
    fn properties_block() {
        assert!(is_properties_block("type:: journal\n\ndate:: 2026-10-25\n"));
        assert!(!is_properties_block("type:: journal\nand some text"));
        assert!(!is_properties_block(""));
        assert!(is_properties_block("#tag"));
        assert!(is_properties_block("type:: person\n#flag"));
        assert!(!is_properties_block("#tag and prose"));
        assert!(!is_properties_block("#tag(1)"));
        assert!(!is_properties_block("#123"));
        assert!(is_properties_block(
            "type:: property\n##transitive\n##values(#type(place))"
        ));
    }

    #[test]
    fn meta_lines() {
        let meta = |s: &str, k: &str, p: Option<&str>| {
            assert_eq!(
                parse_meta_line(s),
                Some((k.to_owned(), p.map(str::to_owned))),
                "{s:?}"
            );
        };
        meta("##transitive", "transitive", None);
        meta("  ##inverse( children ) ", "inverse", Some("children"));
        meta("##values()", "values", None);
        meta("##[[odd key]](x)", "odd key", Some("x"));
        meta(
            "##values(#type(city) | #type(country))",
            "values",
            Some("#type(city) | #type(country)"),
        );
        meta(r"##values(a \) b)", "values", Some(r"a \) b"));
        meta(
            r#"##values("a)" | [[b)]])"#,
            "values",
            Some(r#""a)" | [[b)]]"#),
        );
        meta(r#"##values(O"Brien)"#, "values", Some(r#"O"Brien"#));
        // Found by fuzzing: a form feed isn't space between terms, so the
        // quote is part of the name and doesn't start a quoted one.
        meta("##values(a ~\u{c}\"b)", "values", Some("a ~\u{c}\"b"));
        // Found by fuzzing: the same at the start of the parameter. Trimming
        // the form feeds would leave a quote there, which reads as an
        // unterminated quoted name once the line is formatted and read again.
        meta(
            "##values(\u{c}\"\0 (place) | [[Earth]])",
            "values",
            Some("\u{c}\"\0 (place) | [[Earth]]"),
        );
        for not in [
            "## heading",
            "##",
            "###x",
            "##x(a",
            "##x(a) b",
            "##x.",
            "##x((a)",
            "a ##x",
            "#x",
        ] {
            assert_eq!(parse_meta_line(not), None, "{not:?}");
        }
    }

    /// A meta-property line isn't prose: the properties in its parameter
    /// aren't the block's, but the links in it are.
    #[test]
    fn meta_lines_in_blocks() {
        let text = "type:: property\n##values(#type([[place]]) | x)\n##transitive";
        let block = parse_block(text);
        assert_eq!(block.properties.len(), 1);
        assert_eq!(
            block
                .meta
                .iter()
                .map(|m| &text[m.span.clone()])
                .collect::<Vec<_>>(),
            ["##values(#type([[place]]) | x)", "##transitive"]
        );
        assert_eq!(block.links, [link("place")]);
        assert!(parse_block("##values(#k(a) &)").links.is_empty());
    }

    #[test]
    fn meta_value_lines() {
        let value = |line: &str| parse_meta_value_line(line);
        assert_eq!(
            value("##tag:: person"),
            Some(("tag".into(), Some(text("person"))))
        );
        assert_eq!(
            value("  ##tag:: [person, [[Ann]]] "),
            Some((
                "tag".into(),
                Some(Value::List(vec![text("person"), Value::Link(link("Ann"))]))
            ))
        );
        assert_eq!(value("##journal::"), Some(("journal".into(), None)));
        assert_eq!(
            value("##[[odd key]]:: x"),
            Some(("odd key".into(), Some(text("x"))))
        );
        for not in [
            "##tag",
            "##tag(x)",
            "##tag::x",
            "##tag :: x",
            "tag:: x",
            "# #tag:: x",
            "###tag:: x",
            "##:: x",
            "##k(a):: b",
        ] {
            assert_eq!(value(not), None, "{not:?}");
        }
        // Neither form is the other's.
        assert_eq!(parse_meta_line("##tag:: x"), None);
        assert_eq!(parse_property_line("##tag:: x"), None);
        assert!(is_properties_block("##tag:: [a, b]\nnote:: x"));
    }

    #[test]
    fn meta_value_lines_in_blocks() {
        let text = "type:: x\n##tag:: [[Ann]]\n##values(#a)\n##journal::";
        let block = parse_block(text);
        let seen: Vec<_> = block
            .meta
            .iter()
            .map(|m| {
                (
                    &text[m.span.clone()],
                    m.value.is_some(),
                    m.parameter.is_some(),
                )
            })
            .collect();
        assert_eq!(
            seen,
            [
                ("##tag:: [[Ann]]", true, false),
                ("##values(#a)", false, true),
                ("##journal::", false, false),
            ]
        );
        assert_eq!(block.links, [link("Ann")]);
        assert_eq!(block.properties.len(), 1, "a ## line is not a property");
    }

    /// Found by fuzzing: `##key::` with nothing after it has neither a value
    /// nor a parameter, so it can't be told from `##key` by what it holds. It
    /// is still the value form, which is the one that reads it.
    #[test]
    fn a_meta_value_line_may_have_no_value() {
        let block = parse_block("##k:: \n##[[a::b]]");
        let seen: Vec<_> = block
            .meta
            .iter()
            .map(|m| (m.key.as_str(), m.value.is_some(), m.parameter.is_some()))
            .collect();
        assert_eq!(seen, [("k", false, false), ("a::b", false, false)]);
        assert_eq!(parse_meta_line("##k:: "), None);
        assert_eq!(parse_meta_value_line("##k:: "), Some(("k".into(), None)));
        assert_eq!(
            parse_meta_line("##[[a::b]]"),
            Some(("a::b".into(), None)),
            "a `::` inside a bracketed key is part of the key"
        );
    }

    #[test]
    fn a_properties_block_is_known_by_its_first_line() {
        let yes = |text: &str| parse_properties_block(text).is_some();
        assert!(yes("type:: person"));
        assert!(
            yes("\n\n  born:: 1970\nsome prose"),
            "blank lines don't count"
        );
        assert!(yes("#person"));
        assert!(yes("#a #[[b c]]"));
        assert!(yes("##journal(2026-10-09)"));
        assert!(yes("##tag:: [a]"));
        assert!(!yes("prose\ntype:: person"), "prose first");
        assert!(!yes("#todo buy milk"), "a tag in prose");
        assert!(!yes("#a."), "a full stop isn't part of a tag");
        assert!(!yes("#123"));
        assert!(!yes(""));
        assert!(!yes(" \n\t\n"));
        assert!(!yes("- bullet:: no"));
    }

    #[test]
    fn a_blob_line_first_makes_a_blob_not_a_properties_block() {
        let yes = |text: &str| parse_properties_block(text).is_some();
        assert!(!yes("##blob(abc, image/png, 1)"));
        assert!(!yes("\n  ##Blob(x)\ncaption"));
        assert!(!yes("##blob:: x"));
        assert!(!yes("##blob"));
        // Anywhere else, or another key, it is as any `##` line is.
        assert!(yes("kind:: a\n##blob(abc, image/png, 1)"));
        assert!(yes("##blobs(x)"));
        assert!(yes("##block(x)"));
    }

    #[test]
    fn prose_in_a_properties_block_is_stray() {
        let text = "type:: person\nMet at [[Summit]] #oops\n#a #b\r\n\nborn:: 1970";
        let found = parse_properties_block(text).unwrap();
        let stray: Vec<_> = found.stray.iter().map(|r| &text[r.clone()]).collect();
        assert_eq!(stray, ["Met at [[Summit]] #oops"]);
        let keys: Vec<_> = found
            .block
            .properties
            .iter()
            .map(|p| p.key.as_str())
            .collect();
        assert_eq!(
            keys,
            ["type", "a", "b", "born"],
            "the stray line adds nothing"
        );
        assert!(
            found.block.links.is_empty(),
            "a stray line's links aren't read"
        );
        let spans: Vec<_> = found
            .block
            .properties
            .iter()
            .map(|p| &text[p.span.clone()])
            .collect();
        assert_eq!(spans, ["type:: person", "#a", "#b", "born:: 1970"]);
    }

    #[test]
    fn tag_lines() {
        for yes in ["#a", "  #a  #b ", "#a#b", "#[[b c]] #d", "\t#a"] {
            assert!(is_tags_line(yes), "{yes:?}");
        }
        for no in ["", "  ", "a", "#a b", "#a. #b", "# a", "#", "#a x#b", "##a"] {
            assert!(!is_tags_line(no), "{no:?}");
        }
    }

    #[test]
    fn bare_keys() {
        assert!(is_bare_key("status"));
        assert!(is_bare_key("area/work"));
        assert!(!is_bare_key("due date"));
        assert!(!is_bare_key("trailing."));
        assert!(!is_bare_key("123"));
        assert!(!is_bare_key(""));
    }
}
