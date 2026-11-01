//! Property tests: anything the formatter writes, the parser reads back
//! exactly, alone or embedded in surrounding prose.
//!
//! Robustness against arbitrary input (no panics, sane spans) is covered by
//! the fuzz targets in `fuzz/`.

use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;
use quiddity_syntax::{
    Link, PropertyStyle, Qualifier, Query, Value, format_inline_property, format_meta_line,
    format_property_line, format_query, is_properties_block, parse_block, parse_meta_line,
    parse_property_line, parse_query,
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
    Inline(String, Option<Value>),
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
        (inline_key(), option::of(value())).prop_map(|(k, v)| Fragment::Inline(k, v)),
    ];
    prop_oneof![
        (line_key(), option::of(value())).prop_map(|(k, v)| Line::Property(k, v)),
        vec(fragment, 1..5).prop_map(Line::Mixed),
    ]
}

proptest! {
    #[test]
    fn inline_properties_round_trip(key in inline_key(), value in option::of(value())) {
        let src = format_inline_property(&key, value.as_ref());
        let block = parse_block(&src);
        prop_assert_eq!(block.properties.len(), 1, "{}", src);
        let p = &block.properties[0];
        prop_assert_eq!(&p.key, &key);
        prop_assert_eq!(&p.value, &value);
        prop_assert_eq!(p.span.clone(), 0..src.len());

        // References come from the value's structure: links and block refs
        // at the top level or in a list, but not inside text or qualifiers.
        let (mut links, mut refs) = (Vec::new(), Vec::new());
        let items = match &value {
            Some(Value::List(items)) => items.clone(),
            Some(v) => vec![v.clone()],
            None => Vec::new(),
        };
        for item in items {
            match item {
                Value::Link(l) => links.push(l),
                Value::BlockRef(id) => refs.push(id),
                _ => {}
            }
        }
        prop_assert_eq!(&block.links, &links, "{}", src);
        prop_assert_eq!(&block.block_refs, &refs, "{}", src);
    }

    #[test]
    fn property_lines_round_trip(key in line_key(), value in option::of(value())) {
        let line = format_property_line(&key, value.as_ref());
        prop_assert!(!line.contains(['\n', '\r']), "{:?}", line);
        prop_assert_eq!(parse_property_line(&line), Some((key, value)), "{}", line);
    }

    /// Properties are found in context: among prose, next to each other, and
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
                    // Without a value it's written `#key`, which in a block
                    // is an inline property like any other.
                    let style = if value.is_some() { PropertyStyle::Line } else { PropertyStyle::Inline };
                    expected.push((key.clone(), value.clone(), style, written.clone()));
                    text.push_str(&written);
                }
                Line::Mixed(fragments) => {
                    let mut prev_inline = false;
                    for (j, fragment) in fragments.iter().enumerate() {
                        let is_inline = matches!(fragment, Fragment::Inline(..));
                        // Inline properties may abut each other; anything
                        // else needs a space.
                        if j > 0 && !(tight && prev_inline && is_inline) {
                            text.push(' ');
                        }
                        match fragment {
                            Fragment::Prose(s) => text.push_str(s),
                            Fragment::Inline(key, value) => {
                                let written = format_inline_property(key, value.as_ref());
                                expected.push((key.clone(), value.clone(), PropertyStyle::Inline, written.clone()));
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
