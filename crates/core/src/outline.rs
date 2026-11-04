//! The text format: each entity as a Markdown outline, in the style of
//! Logseq's original file format.
//!
//! ```markdown
//! title:: Jane
//! type:: person
//!
//! - Met at [[Summit#type(event)]]
//!   a second line of the same block
//!     - a child block (indented with a tab in the file)
//! ```
//!
//! The lines before the first bullet are the entity's name (`title::`) and
//! properties. Bullets are indented with tabs, and continuation lines with
//! the bullet's tabs plus two spaces.

use quiddity_syntax::{
    Value, format_property_line, is_properties_block, normalise_name, parse_property_line,
};

use crate::{BlockId, Graph, Result};

/// How bullets are indented in a file being read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indent {
    /// Exactly as Quiddity writes it: one tab per level. Lossless.
    Tabs,
    /// Whatever another tool wrote: any mix of tabs and spaces, nesting
    /// judged by width. Best effort.
    Auto,
}

/// An entity as text, before it is added to a graph.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outline {
    pub title: Option<String>,
    /// The text of the entity's properties block.
    pub properties: Option<String>,
    pub blocks: Vec<OutlineBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineBlock {
    pub text: String,
    pub children: Vec<OutlineBlock>,
}

/// The outline of an entity in the graph.
pub fn outline_of(graph: &Graph, entity: BlockId) -> Result<Outline> {
    // The properties block goes in the file's header, which can't hold
    // children. One that has children is written as the first bullet
    // instead, and reads back as the same graph.
    let props = graph
        .properties_block(entity)?
        .filter(|&p| graph.children(p).is_empty());
    let blocks = graph
        .children(entity)
        .into_iter()
        .filter(|&b| Some(b) != props)
        .map(|b| outline_block(graph, b))
        .collect::<Result<_>>()?;
    Ok(Outline {
        title: Some(graph.text(entity)?),
        properties: props.map(|p| graph.text(p)).transpose()?,
        blocks,
    })
}

fn outline_block(graph: &Graph, block: BlockId) -> Result<OutlineBlock> {
    Ok(OutlineBlock {
        text: graph.text(block)?,
        children: graph
            .children(block)
            .into_iter()
            .map(|b| outline_block(graph, b))
            .collect::<Result<_>>()?,
    })
}

/// Writes an entity as Markdown.
pub fn export_entity(graph: &Graph, entity: BlockId) -> Result<String> {
    Ok(format_outline(&outline_of(graph, entity)?))
}

/// Adds an outline to the graph as a new entity, named `fallback_name` if the
/// outline has no title.
pub fn import_outline(graph: &Graph, outline: &Outline, fallback_name: &str) -> Result<BlockId> {
    let name = outline.title.as_deref().unwrap_or(fallback_name);
    let entity = graph.create_block(None, None, name)?;
    if let Some(props) = &outline.properties {
        graph.create_block(Some(entity), None, props)?;
    }
    fn add(graph: &Graph, parent: BlockId, blocks: &[OutlineBlock]) -> Result<()> {
        for block in blocks {
            let id = graph.create_block(Some(parent), None, &block.text)?;
            add(graph, id, &block.children)?;
        }
        Ok(())
    }
    add(graph, entity, &outline.blocks)?;
    graph.commit();
    Ok(entity)
}

pub fn format_outline(outline: &Outline) -> String {
    let mut out = String::new();
    if let Some(title) = &outline.title {
        out.push_str(&format_property_line(
            "title",
            Some(&Value::Text(title.clone())),
        ));
        out.push('\n');
    }
    if let Some(props) = &outline.properties {
        for line in props.split('\n') {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !out.is_empty() {
        out.push('\n');
    }
    fn write(out: &mut String, block: &OutlineBlock, depth: usize) {
        let tabs = "\t".repeat(depth);
        let mut lines = block.text.split('\n');
        match lines.next().unwrap_or("") {
            "" => out.push_str(&format!("{tabs}-\n")),
            first => out.push_str(&format!("{tabs}- {first}\n")),
        }
        for line in lines {
            out.push_str(&format!("{tabs}  {line}\n"));
        }
        for child in &block.children {
            write(out, child, depth + 1);
        }
    }
    for block in &outline.blocks {
        write(&mut out, block, 0);
    }
    out
}

/// One bullet's worth of lines while parsing.
struct Pending {
    depth: usize,
    /// Column at which the bullet's content starts (for [`Indent::Auto`]).
    content_col: usize,
    /// Lines, each flagged if it was blank in the source and so may be
    /// dropped from the end of the block.
    lines: Vec<(String, bool)>,
    fence: Option<(char, usize)>,
}

impl Pending {
    fn push(&mut self, line: String, blank: bool) {
        if let Some((c, n, closes)) = fence_marker(&line) {
            match self.fence {
                None => self.fence = Some((c, n)),
                Some((open, len)) if closes && c == open && n >= len => self.fence = None,
                Some(_) => {}
            }
        }
        self.lines.push((line, blank));
    }

    /// The block's text, without blank lines after it. (The bullet's own
    /// line is never flagged blank, so it always stays.)
    fn text(mut self) -> (usize, String) {
        while self.lines.last().is_some_and(|(_, blank)| *blank) {
            self.lines.pop();
        }
        let lines: Vec<_> = self.lines.into_iter().map(|(l, _)| l).collect();
        (self.depth, lines.join("\n"))
    }
}

pub fn parse_outline(src: &str, indent: Indent) -> Outline {
    // Quiddity's own files keep a `\r` that is part of a block's text;
    // anything else may have Windows line endings.
    let lines: Vec<&str> = match indent {
        Indent::Tabs => src.strip_suffix('\n').unwrap_or(src).split('\n').collect(),
        Indent::Auto => src.lines().collect(),
    };
    let first_bullet = lines
        .iter()
        .position(|l| bullet(l, indent).is_some())
        .unwrap_or(lines.len());

    let mut flat: Vec<(usize, String)> = Vec::new();
    let mut widths: Vec<usize> = Vec::new();
    let mut current: Option<Pending> = None;
    for line in &lines[first_bullet..] {
        let in_fence = current.as_ref().is_some_and(|p| p.fence.is_some());
        let is_bullet = match indent {
            Indent::Tabs => bullet(line, indent),
            Indent::Auto if !in_fence => bullet(line, indent),
            Indent::Auto => None,
        };
        if let Some((width, content)) = is_bullet {
            let depth = match indent {
                Indent::Tabs => width.min(current.as_ref().map_or(0, |p| p.depth + 1)),
                Indent::Auto => {
                    while widths.last().is_some_and(|&w| w >= width) {
                        widths.pop();
                    }
                    widths.push(width);
                    widths.len() - 1
                }
            };
            flat.extend(current.take().map(Pending::text));
            let mut pending = Pending {
                depth,
                content_col: width + 2,
                lines: Vec::new(),
                fence: None,
            };
            pending.push(content.to_owned(), false);
            current = Some(pending);
        } else if let Some(pending) = current.as_mut() {
            let (stripped, blank) = match indent {
                Indent::Tabs => (strip_tabs(line, pending.depth), line.is_empty()),
                Indent::Auto => (
                    strip_columns(line, pending.content_col),
                    line.trim().is_empty(),
                ),
            };
            pending.push(stripped.to_owned(), blank);
        }
    }
    flat.extend(current.take().map(Pending::text));

    let mut outline = Outline {
        blocks: build_tree(flat),
        ..Outline::default()
    };
    let pre: Vec<&str> = lines[..first_bullet].to_vec();
    let (title, rest) = take_title(&pre);
    outline.title = title;
    if is_properties_block(&rest) {
        outline.properties = Some(rest);
    } else if !rest.is_empty() {
        outline.blocks.insert(
            0,
            OutlineBlock {
                text: rest,
                children: Vec::new(),
            },
        );
    }
    // Logseq sometimes writes page properties as the first bullet instead.
    if outline.title.is_none()
        && outline.properties.is_none()
        && let Some(first) = outline
            .blocks
            .first_mut()
            .filter(|b| is_properties_block(&b.text))
    {
        // Line endings were dealt with above; a `\r` left is the text's own.
        let lines: Vec<&str> = first.text.split('\n').collect();
        let (title, rest) = take_title(&lines);
        // Without a title there is nothing to lift, and `take_title` trims
        // blank lines that are the block's own text.
        if title.is_some() {
            outline.title = title;
            if rest.is_empty() && first.children.is_empty() {
                outline.blocks.remove(0);
            } else {
                first.text = rest;
            }
        }
    }
    outline
}

/// Splits the first `title::` line from the others, which are trimmed of
/// blank lines at either end.
fn take_title(lines: &[&str]) -> (Option<String>, String) {
    let mut title = None;
    let mut rest = Vec::new();
    for line in lines {
        match parse_property_line(line) {
            Some((key, value)) if title.is_none() && normalise_name(&key) == "title" => {
                title = Some(match value {
                    Some(Value::Text(s)) => s,
                    Some(Value::Link(link)) => link.name,
                    Some(other) => quiddity_syntax::format_value(&other),
                    None => String::new(),
                });
            }
            _ => rest.push(*line),
        }
    }
    let start = rest
        .iter()
        .position(|l| !l.trim().is_empty())
        .unwrap_or(rest.len());
    let end = rest
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .map_or(start, |i| i + 1);
    (title, rest[start..end].join("\n"))
}

fn build_tree(flat: Vec<(usize, String)>) -> Vec<OutlineBlock> {
    let mut roots = Vec::new();
    let mut stack: Vec<OutlineBlock> = Vec::new();
    fn close(stack: &mut Vec<OutlineBlock>, roots: &mut Vec<OutlineBlock>) {
        let block = stack.pop().expect("closing an open block");
        match stack.last_mut() {
            Some(parent) => parent.children.push(block),
            None => roots.push(block),
        }
    }
    for (depth, text) in flat {
        while stack.len() > depth {
            close(&mut stack, &mut roots);
        }
        stack.push(OutlineBlock {
            text,
            children: Vec::new(),
        });
    }
    while !stack.is_empty() {
        close(&mut stack, &mut roots);
    }
    roots
}

/// If `line` is a bullet, its indentation (tabs, or columns for
/// [`Indent::Auto`]) and content.
fn bullet(line: &str, indent: Indent) -> Option<(usize, &str)> {
    let (width, rest) = match indent {
        Indent::Tabs => {
            let rest = line.trim_start_matches('\t');
            (line.len() - rest.len(), rest)
        }
        Indent::Auto => {
            let rest = line.trim_start();
            (columns(&line[..line.len() - rest.len()]), rest)
        }
    };
    match rest {
        "-" => Some((width, "")),
        _ => rest.strip_prefix("- ").map(|content| (width, content)),
    }
}

fn columns(ws: &str) -> usize {
    ws.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum()
}

/// Removes a continuation line's indentation: `depth` tabs then two spaces,
/// or as much of that as is there.
fn strip_tabs(line: &str, depth: usize) -> &str {
    let mut rest = line;
    for _ in 0..depth {
        rest = rest.strip_prefix('\t').unwrap_or(rest);
    }
    for _ in 0..2 {
        rest = rest.strip_prefix(' ').unwrap_or(rest);
    }
    rest
}

fn strip_columns(line: &str, cols: usize) -> &str {
    let mut seen = 0;
    for (i, c) in line.char_indices() {
        if seen >= cols || !matches!(c, ' ' | '\t') {
            return &line[i..];
        }
        seen += columns(&line[i..i + 1]);
    }
    ""
}

fn fence_marker(line: &str) -> Option<(char, usize, bool)> {
    let t = line.trim_start();
    let c = t.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let n = t.len() - t.trim_start_matches(c).len();
    (n >= 3).then(|| (c, n, t[n..].trim().is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(text: &str, children: Vec<OutlineBlock>) -> OutlineBlock {
        OutlineBlock {
            text: text.into(),
            children,
        }
    }

    // Round trips are property-tested in tests/outline.rs.
    #[test]
    fn format() {
        let outline = Outline {
            title: Some("Jane".into()),
            properties: Some("type:: person".into()),
            blocks: vec![block("a\nb", vec![block("c", vec![])])],
        };
        assert_eq!(
            format_outline(&outline),
            "title:: Jane\ntype:: person\n\n- a\n  b\n\t- c\n"
        );
    }

    #[test]
    fn logseq_files() {
        let src = "title:: My Page\nalias:: Mine\n\n- one\n  continued\n    - two\n        - three\n    - four\n- five\n";
        let outline = parse_outline(src, Indent::Auto);
        assert_eq!(outline.title.as_deref(), Some("My Page"));
        assert_eq!(outline.properties.as_deref(), Some("alias:: Mine"));
        assert_eq!(
            outline.blocks,
            vec![
                block(
                    "one\ncontinued",
                    vec![
                        block("two", vec![block("three", vec![])]),
                        block("four", vec![])
                    ]
                ),
                block("five", vec![]),
            ]
        );

        let first_bullet_props =
            parse_outline("- title:: Other\n  tags:: x\n- body\n", Indent::Auto);
        assert_eq!(first_bullet_props.title.as_deref(), Some("Other"));
        assert_eq!(
            first_bullet_props.blocks,
            vec![block("tags:: x", vec![]), block("body", vec![])]
        );
        // A `\r` in Quiddity's own format is part of the text, even there.
        let kept = parse_outline("- a:: x\r\n  b:: y\n", Indent::Tabs);
        assert_eq!(kept.blocks, vec![block("a:: x\r\nb:: y", vec![])]);
    }

    #[test]
    fn a_first_bullet_of_properties_without_a_title_is_left_alone() {
        // Blank lines in its text are the block's own, not padding to trim.
        let src = "- \n  \rc::\n";
        let outline = parse_outline(src, Indent::Tabs);
        assert_eq!(outline.title, None);
        assert_eq!(outline.blocks, vec![block("\n\rc::", vec![])]);
    }

    #[test]
    fn auto_mode_respects_fences() {
        let src = "- code:\n  ```\n  - not a bullet\n  ```\n- next\n";
        let outline = parse_outline(src, Indent::Auto);
        assert_eq!(
            outline.blocks,
            vec![
                block("code:\n```\n- not a bullet\n```", vec![]),
                block("next", vec![])
            ]
        );
        // Only a bare run of the same character, at least as long, closes.
        for close in ["~~~", "``` x", "```"] {
            let src = format!("- a\n  ````\n  {close}\n  - still code\n  ````\n- b\n");
            let outline = parse_outline(&src, Indent::Auto);
            assert_eq!(outline.blocks.len(), 2, "{src}");
        }
    }

    #[test]
    fn blank_lines_and_indentation() {
        let tabs = parse_outline("- a\n\n  c\n\n\n- b\n", Indent::Tabs);
        assert_eq!(
            tabs.blocks,
            vec![block("a\n\nc", vec![]), block("b", vec![])],
            "blank lines end a block only at its end"
        );
        let auto = parse_outline("- a\n     deeper\n  \tb\n\n- c\n", Indent::Auto);
        assert_eq!(
            auto.blocks,
            vec![block("a\n   deeper\n\tb", vec![]), block("c", vec![])],
            "only the bullet's own indentation is removed"
        );
    }

    #[test]
    fn untitled_text_before_bullets_is_a_block() {
        let outline = parse_outline("Some prose.\n\n- a\n", Indent::Auto);
        assert_eq!(outline.title, None);
        assert_eq!(
            outline.blocks,
            vec![block("Some prose.", vec![]), block("a", vec![])]
        );
    }
}
