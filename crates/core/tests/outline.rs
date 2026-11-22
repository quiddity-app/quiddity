//! The outline text format, independent of the graph.

mod common;

use std::fmt::Write as _;

use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;
use quiddity_core::outline::{Indent, Outline, OutlineBlock, format_outline, parse_outline};

fn blocks(
    text: impl Strategy<Value = String> + Clone + 'static,
) -> impl Strategy<Value = Vec<OutlineBlock>> {
    let leaf = text.clone().prop_map(|text| OutlineBlock {
        text,
        children: Vec::new(),
    });
    let block = leaf.prop_recursive(3, 24, 4, move |inner| {
        (text.clone(), vec(inner, 0..4))
            .prop_map(|(text, children)| OutlineBlock { text, children })
    });
    vec(block, 0..5)
}

fn outline(
    text: impl Strategy<Value = String> + Clone + 'static,
) -> impl Strategy<Value = Outline> {
    (
        any::<String>(),
        option::of(common::property_lines()),
        blocks(text),
    )
        .prop_map(|(title, properties, blocks)| Outline {
            title: Some(title),
            properties,
            blocks,
        })
}

/// Writes an outline as another tool might: spaces rather than tabs.
fn with_spaces(outline: &Outline, per_level: usize) -> String {
    fn write(out: &mut String, block: &OutlineBlock, depth: usize, per_level: usize) {
        let indent = " ".repeat(depth * per_level);
        for (i, line) in block.text.split('\n').enumerate() {
            let marker = if i == 0 { "- " } else { "  " };
            writeln!(out, "{indent}{marker}{line}").expect("formatting into a String failed");
        }
        for child in &block.children {
            write(out, child, depth + 1, per_level);
        }
    }
    let mut out = String::new();
    if let Some(title) = &outline.title {
        writeln!(out, "title:: {title}").expect("formatting into a String failed");
    }
    if let Some(props) = &outline.properties {
        writeln!(out, "{props}").expect("formatting into a String failed");
    }
    out.push('\n');
    for block in &outline.blocks {
        write(&mut out, block, 0, per_level);
    }
    out
}

proptest! {
    /// Quiddity's own format is lossless for any text at all.
    #[test]
    fn own_format_round_trips(outline in outline(common::text())) {
        let text = format_outline(&outline);
        prop_assert_eq!(parse_outline(&text, Indent::Tabs), outline, "{}", text);
    }

    /// Files indented with spaces nest by width. Their text is limited to
    /// lines a person might write, since a continuation line starting with
    /// `- ` is a bullet in such files.
    #[test]
    fn space_indented_files_nest_by_width(
        outline in outline("[a-z#:()\\[\\]][a-z#:()\\[\\] ]{0,8}(\n[a-z#:()\\[\\]][a-z ]{0,8}){0,2}"),
        title in "[A-Za-z]([A-Za-z ]{0,8}[A-Za-z])?",
        per_level in 2usize..5,
    ) {
        let outline = Outline { title: Some(title), ..outline };
        let text = with_spaces(&outline, per_level);
        prop_assert_eq!(parse_outline(&text, Indent::Auto), outline, "{}", text);
    }
}
