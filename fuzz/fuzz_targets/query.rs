//! Arbitrary text through the query parser.
//!
//! - Parsing never panics.
//! - A query that parses is written so that it reads back the same, and
//!   writing it again changes nothing.
//! - Text that parses as a query is read whole as a meta-property's
//!   parameter, so the two parsers agree on where a query ends, unless it
//!   ends in a backslash, which escapes the `)`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use quiddity_syntax::{format_meta_line, format_query, parse_meta_line, parse_query};

fuzz_target!(|text: &str| {
    let Ok(query) = parse_query(text) else {
        return;
    };
    let written = format_query(&query);
    let again = parse_query(&written);
    assert_eq!(again.as_ref(), Ok(&query), "{written:?}");
    assert_eq!(format_query(&again.unwrap()), written);

    let line = format_meta_line("k", Some(&written));
    assert_eq!(
        parse_meta_line(&line),
        Some(("k".to_owned(), Some(written.clone()))),
        "{line:?}"
    );
    let trimmed = text.trim();
    if trimmed == text && !text.contains(['\n', '\r']) && !text.ends_with('\\') {
        assert_eq!(
            parse_meta_line(&format!("##k({text})")),
            Some(("k".to_owned(), Some(text.to_owned()))),
            "{text:?}"
        );
    }
});
