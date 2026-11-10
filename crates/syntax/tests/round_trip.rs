//! Property tests: anything the formatter writes, the parser reads back
//! exactly, alone or embedded in surrounding prose.
//!
//! Robustness against arbitrary input (no panics, sane spans) is covered by
//! the fuzz targets in `fuzz/`.

use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;
use quiddity_syntax::{
    Link, PropertyStyle, Qualifier, Query, Value, format_meta_line, format_meta_value_line,
    format_property_line, format_query, format_tag, is_properties_block, parse_block,
    parse_meta_line, parse_meta_value_line, parse_properties_block, parse_property_line,
    parse_query,
};

/// Text weighted towards the characters that mean something to the parser.
fn any_text() -> impl Strategy<Value = String> {
    prop_oneof![r#"[ a-z#()\[\],"\\:\n\r]{0,12}"#, any::<String>(),]
}

/// A key usable after `#`: trimmed, no brackets, no line breaks.
fn inline_key() -> impl Strategy<Value = String> {
    r"[^\[\]\n\r]{1,10}".prop_filter("trimmed", |k| !k.is_empty() && k.trim() == k)
}

/// A key usable in a `key:: value` line.
fn line_key() -> impl Strategy<Value = String> {
    r"[\p{L}\p{N}_][^:#\[\]()\n\r]{0,10}".prop_filter("no trailing space", |k| k.trim_end() == k)
}

/// A link name, weighted towards the characters that need escaping. Names
/// are single lines: they're titles, written with whitespace collapsed.
fn link_name() -> impl Strategy<Value = String> {
    prop_oneof![r#"[a-z_#\[\]\\ ]{1,8}"#, r"[^\n\r]{1,12}"]
        .prop_filter("trimmed", |n| !n.is_empty() && n.trim() == n)
}

fn block_ref_id() -> impl Strategy<Value = String> {
    r"[^\s()]{1,12}"
}

fn link(value: impl Strategy<Value = Value> + Clone) -> impl Strategy<Value = Link> {
    let qualifier_value = prop_oneof![value.clone(), vec(value, 0..3).prop_map(Value::List)];
    let qualifier = (inline_key(), option::of(qualifier_value))
        .prop_map(|(key, value)| Qualifier { key, value });
    (link_name(), vec(qualifier, 0..3)).prop_map(|(name, qualifiers)| Link { name, qualifiers })
}

/// Any value: text, block refs and links (with nested qualifier values), or
/// a flat list of those.
fn value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        any_text().prop_map(Value::Text),
        block_ref_id().prop_map(Value::BlockRef),
    ];
    let scalar = leaf.prop_recursive(2, 8, 3, |inner| link(inner).prop_map(Value::Link));
    prop_oneof![
        3 => scalar.clone(),
        1 => vec(scalar, 0..4).prop_map(Value::List),
    ]
}

/// A piece of a block's text, with the property it should parse to, if any.
#[derive(Debug, Clone)]
enum Fragment {
    Prose(String),
    Tag(String),
}

/// One line of a block: either a property line, or prose with inline
/// properties scattered through it.
#[derive(Debug, Clone)]
enum Line {
    Property(String, Option<Value>),
    Mixed(Vec<Fragment>),
}

fn line() -> impl Strategy<Value = Line> {
    let fragment = prop_oneof![
        "[A-Za-z][A-Za-z .,!?]{0,10}".prop_map(Fragment::Prose),
        inline_key().prop_map(Fragment::Tag),
    ];
    prop_oneof![
        (line_key(), option::of(value())).prop_map(|(k, v)| Line::Property(k, v)),
        vec(fragment, 1..5).prop_map(Line::Mixed),
    ]
}

proptest! {
    #[test]
    fn tags_round_trip(key in inline_key()) {
        let src = format_tag(&key);
        let block = parse_block(&src);
        prop_assert_eq!(block.properties.len(), 1, "{}", src);
        let p = &block.properties[0];
        prop_assert_eq!(&p.key, &key);
        prop_assert_eq!(&p.value, &None);
        prop_assert_eq!(p.style, PropertyStyle::Inline);
        prop_assert_eq!(p.span.clone(), 0..src.len());
        prop_assert!(block.links.is_empty(), "{}", src);
    }

    /// Whatever follows a tag is text, so it changes nothing about the tag. A
    /// delimiter comes first because key characters would extend a bare key.
    #[test]
    fn a_tag_ignores_what_follows(
        key in inline_key(),
        gap in prop_oneof![Just(" "), Just("(")],
        after in any_text(),
    ) {
        let tag = format_tag(&key);
        let block = parse_block(&format!("{tag}{gap}{after}"));
        let first = &block.properties[0];
        prop_assert_eq!((&first.key, first.span.clone()), (&key, 0..tag.len()));
    }

    #[test]
    fn property_lines_round_trip(key in line_key(), value in option::of(value())) {
        let line = format_property_line(&key, value.as_ref());
        prop_assert!(!line.contains(['\n', '\r']), "{:?}", line);
        prop_assert_eq!(parse_property_line(&line), Some((key, value)), "{}", line);
    }

    /// Properties and tags are found in context: among prose, next to each other, and
    /// on separate lines, with spans pointing at exactly what was written.
    #[test]
    fn properties_in_context(lines in vec(line(), 1..5), tight in any::<bool>()) {
        let mut text = String::new();
        let mut expected = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                text.push('\n');
            }
            match line {
                Line::Property(key, value) => {
                    let written = format_property_line(key, value.as_ref());
                    expected.push((key.clone(), value.clone(), PropertyStyle::Line, written.clone()));
                    text.push_str(&written);
                }
                Line::Mixed(fragments) => {
                    let mut prev_inline = false;
                    for (j, fragment) in fragments.iter().enumerate() {
                        let is_inline = matches!(fragment, Fragment::Tag(..));
                        // Inline properties may abut each other; anything
                        // else needs a space.
                        if j > 0 && !(tight && prev_inline && is_inline) {
                            text.push(' ');
                        }
                        match fragment {
                            Fragment::Prose(s) => text.push_str(s),
                            Fragment::Tag(key) => {
                                let written = format_tag(key);
                                expected.push((key.clone(), None, PropertyStyle::Inline, written.clone()));
                                text.push_str(&written);
                            }
                        }
                        prev_inline = is_inline;
                    }
                }
            }
        }
        let found: Vec<_> = parse_block(&text)
            .properties
            .into_iter()
            .map(|p| (p.key, p.value, p.style, text[p.span].to_owned()))
            .collect();
        prop_assert_eq!(found, expected, "{}", text);
    }
}

/// A query, with names weighted towards the characters that mean something
/// to the query parser.
fn query() -> impl Strategy<Value = Query> {
    let name = prop_oneof![
        3 => r#"[ a-z&|~()#\[\]"\\]{0,8}"#,
        1 => any::<String>(),
    ];
    let leaf = prop_oneof![
        4 => name.prop_map(Query::Name),
        1 => link(any_text().prop_map(Value::Text).boxed()).prop_map(Query::Link),
        2 => inline_key().prop_map(|key| Query::Has { key, value: None }),
    ];
    leaf.prop_recursive(4, 16, 2, |inner| {
        let pair = (inner.clone(), inner.clone()).prop_map(|(a, b)| (Box::new(a), Box::new(b)));
        prop_oneof![
            (inline_key(), inner.clone()).prop_map(|(key, v)| Query::Has {
                key,
                value: Some(Box::new(v))
            }),
            inner.prop_map(|q| Query::Not(Box::new(q))),
            pair.clone().prop_map(|(a, b)| Query::And(a, b)),
            pair.clone().prop_map(|(a, b)| Query::Diff(a, b)),
            pair.prop_map(|(a, b)| Query::Or(a, b)),
        ]
    })
}

proptest! {
    #[test]
    fn queries_round_trip(q in query()) {
        let written = format_query(&q);
        prop_assert!(!written.contains(['\n', '\r']), "one line: {:?}", written);
        prop_assert_eq!(parse_query(&written), Ok(q), "{}", written);
    }

    /// A query is a meta-property's parameter as it is, in a properties
    /// block among property lines, with its links counted as the block's.
    #[test]
    fn meta_lines_round_trip(
        key in inline_key(),
        q in option::of(query()),
        before in option::of((line_key(), value())),
    ) {
        let parameter = q.as_ref().map(format_query);
        let line = format_meta_line(&key, parameter.as_deref());
        prop_assert_eq!(parse_meta_line(&line), Some((key.clone(), parameter.clone())), "{}", line);

        let mut text = String::new();
        if let Some((k, v)) = &before {
            text = format_property_line(k, Some(v)) + "\n";
        }
        let at = text.len();
        text.push_str(&line);
        prop_assert!(is_properties_block(&text), "{}", text);
        let block = parse_block(&text);
        prop_assert_eq!(block.meta.len(), 1, "{}", text);
        prop_assert_eq!(&block.meta[0].key, &key);
        prop_assert_eq!(block.meta[0].span.clone(), at..text.len());
        let links: Vec<Link> = q.iter().flat_map(|q| q.links()).cloned().collect();
        prop_assert!(block.links.ends_with(&links), "{}", text);
    }
}

/// One line of a properties block's text.
#[derive(Debug, Clone)]
enum BlockLine {
    Property(String, Value),
    Tags(Vec<String>),
    Meta(String, Option<String>),
    Prose(String),
}

fn block_line() -> impl Strategy<Value = BlockLine> {
    prop_oneof![
        (line_key(), value()).prop_map(|(k, v)| BlockLine::Property(k, v)),
        vec(inline_key(), 1..4).prop_map(BlockLine::Tags),
        (inline_key(), option::of("[a-z]{1,6}")).prop_map(|(k, p)| BlockLine::Meta(k, p)),
        // Words, which can carry tags and links, but never read as a line of
        // the other kinds: they have no `::` (a key may have spaces in it) and
        // start with a letter, so they aren't tags or `##` lines.
        r"[a-z]{1,5} [a-z#\[\]()]{0,8}".prop_map(BlockLine::Prose),
    ]
}

proptest! {
    /// Prose in a properties block is stray, and the lines around it are read
    /// as they would be alone, so one bad line doesn't hide the rest.
    #[test]
    fn a_properties_block_reads_its_lines_and_flags_the_rest(
        first in prop_oneof![
            (line_key(), value()).prop_map(|(k, v)| BlockLine::Property(k, v)),
            vec(inline_key(), 1..4).prop_map(BlockLine::Tags),
        ],
        rest in vec(block_line(), 0..6),
        blanks in any::<bool>(),
    ) {
        let lines: Vec<_> = std::iter::once(first).chain(rest).collect();
        let mut text = String::new();
        let (mut properties, mut meta, mut stray) = (0, 0, Vec::new());
        for line in &lines {
            let written = match line {
                BlockLine::Property(k, v) => {
                    properties += 1;
                    format_property_line(k, Some(v))
                }
                BlockLine::Tags(keys) => {
                    properties += keys.len();
                    keys.iter().map(|k| format_tag(k)).collect::<Vec<_>>().join(" ")
                }
                BlockLine::Meta(k, p) => {
                    meta += 1;
                    format_meta_line(k, p.as_deref())
                }
                BlockLine::Prose(s) => {
                    stray.push(s.clone());
                    s.clone()
                }
            };
            if blanks && !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&written);
            text.push('\n');
        }
        let found = parse_properties_block(&text);
        prop_assert!(found.is_some(), "{}", text);
        let found = found.unwrap();
        let seen: Vec<_> = found.stray.iter().map(|r| text[r.clone()].to_owned()).collect();
        prop_assert_eq!(seen, stray, "{}", text);
        prop_assert_eq!(found.block.properties.len(), properties, "{}", text);
        prop_assert_eq!(found.block.meta.len(), meta, "{}", text);
        // Nothing read lies within a stray line.
        for r in &found.stray {
            let inside = |at: &std::ops::Range<usize>| at.start < r.end && r.start < at.end;
            prop_assert!(!found.block.properties.iter().any(|p| inside(&p.span)), "{}", text);
            prop_assert!(!found.block.meta.iter().any(|m| inside(&m.span)), "{}", text);
        }
    }

    /// Prose first means it isn't a properties block at all.
    #[test]
    fn prose_first_is_not_a_properties_block(prose in r"[a-z]{1,5} [a-z]{1,5}", rest in "[a-z\n ]{0,12}") {
        let text = format!("{}\n{}", prose, rest);
        prop_assert!(parse_properties_block(&text).is_none(), "{}", text);
    }
}

proptest! {
    /// A `##key:: value` line reads back, alone and in a block, with the links
    /// in its value counted as the block's.
    #[test]
    fn meta_value_lines_round_trip(key in inline_key(), value in value()) {
        let line = format_meta_value_line(&key, Some(&value));
        prop_assert!(!line.contains(['\n', '\r']), "{:?}", line);
        prop_assert_eq!(parse_meta_value_line(&line), Some((key.clone(), Some(value))), "{}", line);
        prop_assert!(is_properties_block(&line), "{}", line);
        let block = parse_block(&line);
        prop_assert_eq!(block.meta.len(), 1, "{}", line);
        prop_assert_eq!(&block.meta[0].key, &key);
        prop_assert_eq!(block.meta[0].span.clone(), 0..line.len());
        prop_assert!(block.properties.is_empty(), "{}", line);
    }
}

proptest! {
    /// Text the query parser accepts is read whole as a meta-property's
    /// parameter: the two agree on where a query ends. A trailing backslash
    /// escapes the `)` after it, so that is the exception; `format_query`
    /// never ends with one.
    #[test]
    fn meta_lines_and_queries_agree(text in r#"[ a-z&|~()#\[\]"\\.]{1,16}"#) {
        let text = text.trim();
        if parse_query(text).is_ok() && !text.ends_with('\\') {
            prop_assert_eq!(
                parse_meta_line(&format!("##k({text})")),
                Some(("k".to_owned(), Some(text.to_owned()))),
                "{}", text
            );
        }
    }
}
