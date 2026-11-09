//! Arbitrary text through the block parser.
//!
//! - Parsing never panics.
//! - Spans are in order, don't overlap, and fall on character boundaries.
//! - Each property can be parsed from its span alone, so a property's
//!   meaning doesn't depend on what follows it.
//! - Each `##` line reads the same alone, and as formatted.
//! - Formatting a parsed property or link reads back as the same thing.
//!   The proptests check this for values built by a generator; here the
//!   values come from whatever the parser makes of arbitrary text.
#![no_main]

use libfuzzer_sys::fuzz_target;
use quiddity_syntax::{
    PropertyStyle, format_link, format_meta_line, format_meta_value_line, format_property_line,
    format_tag, parse_block, parse_meta_line, parse_meta_value_line, parse_property_line,
};

fuzz_target!(|text: &str| {
    let block = parse_block(text);

    let mut end = 0;
    for p in &block.properties {
        assert!(p.span.start >= end, "spans overlap: {:?}", block.properties);
        assert!(text.is_char_boundary(p.span.start) && text.is_char_boundary(p.span.end));
        end = p.span.end;
        let written = &text[p.span.clone()];

        match p.style {
            PropertyStyle::Inline => {
                assert!(written.starts_with('#'));
                let alone = parse_block(written).properties;
                assert_eq!(alone.len(), 1, "{written:?}");
                assert_eq!(
                    (&alone[0].key, &alone[0].value),
                    (&p.key, &p.value),
                    "{written:?}"
                );

                let formatted = format_tag(&p.key);
                let again = parse_block(&formatted).properties;
                assert_eq!(again.len(), 1, "{formatted:?}");
                assert_eq!(
                    (&again[0].key, &again[0].value),
                    (&p.key, &p.value),
                    "{formatted:?}"
                );
            }
            PropertyStyle::Line => {
                let expected = Some((p.key.clone(), p.value.clone()));
                assert_eq!(parse_property_line(written), expected, "{written:?}");
                let formatted = format_property_line(&p.key, p.value.as_ref());
                assert_eq!(parse_property_line(&formatted), expected, "{formatted:?}");
            }
        }
    }

    let mut end = 0;
    for m in &block.meta {
        assert!(m.span.start >= end, "spans overlap: {:?}", block.meta);
        assert!(text.is_char_boundary(m.span.start) && text.is_char_boundary(m.span.end));
        end = m.span.end;
        let written = &text[m.span.clone()];
        // A line is a parameter's or a value's, never both. A `##key::` line
        // with nothing after it has neither, and is the value form.
        if parse_meta_line(written).is_some() {
            let expected = Some((m.key.clone(), m.parameter.clone()));
            assert_eq!(parse_meta_line(written), expected, "{written:?}");
            let formatted = format_meta_line(&m.key, m.parameter.as_deref());
            assert_eq!(parse_meta_line(&formatted), expected, "{formatted:?}");
        } else {
            let expected = Some((m.key.clone(), m.value.clone()));
            assert_eq!(parse_meta_value_line(written), expected, "{written:?}");
            // With no value it is written `##key`, which is the other form.
            let formatted = format_meta_value_line(&m.key, m.value.as_ref());
            if m.value.is_some() {
                assert_eq!(parse_meta_value_line(&formatted), expected, "{formatted:?}");
            } else {
                assert_eq!(parse_meta_line(&formatted), Some((m.key.clone(), None)));
            }
        }
    }

    for link in &block.links {
        let formatted = format_link(link);
        assert_eq!(
            parse_block(&formatted).links,
            [link.clone()],
            "{formatted:?}"
        );
    }
});
