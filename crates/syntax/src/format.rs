use crate::parse::{Link, Value, is_bare_key, parse_block, parse_line_value};

/// Where a value is being written; each context has different delimiters.
#[derive(Debug, Clone, Copy)]
enum Context {
    /// Inside `#key(…)`.
    Paren,
    /// Inside `[…]`.
    ListItem,
    /// After `key:: `.
    Line,
}

/// Formats a property key for use after `#`, bracketing it if necessary.
pub fn format_key(key: &str) -> String {
    if is_bare_key(key) {
        key.to_owned()
    } else {
        format!("[[{key}]]")
    }
}

/// Formats a value for use inside `#key(…)`.
pub fn format_value(value: &Value) -> String {
    format_in(value, Context::Paren)
}

/// Formats `#key` or `#key(value)`.
pub fn format_inline_property(key: &str, value: Option<&Value>) -> String {
    match value {
        Some(v) => format!("#{}({})", format_key(key), format_value(v)),
        None => format!("#{}", format_key(key)),
    }
}

/// Formats a `key:: value` line, or `#key` for a property without a value.
/// The key must be valid as a line key.
pub fn format_property_line(key: &str, value: Option<&Value>) -> String {
    match value {
        Some(v) => format!("{key}:: {}", format_in(v, Context::Line)),
        None => format_inline_property(key, None),
    }
}

/// Formats a `##key` or `##key(parameter)` line. The parameter is written
/// as it is, so it must balance as [`crate::parse_meta_line`] expects, which
/// anything [`crate::format_query`] writes does. One ending in a backslash
/// gets a space before the `)`, which the backslash would otherwise escape.
pub fn format_meta_line(key: &str, parameter: Option<&str>) -> String {
    match parameter {
        Some(p) if p.ends_with('\\') => format!("##{}({p} )", format_key(key)),
        Some(p) => format!("##{}({p})", format_key(key)),
        None => format!("##{}", format_key(key)),
    }
}

/// Formats a link, escaping its name only where necessary.
pub fn format_link(link: &Link) -> String {
    let qualifiers: String = link
        .qualifiers
        .iter()
        .map(|q| format_inline_property(&q.key, q.value.as_ref()))
        .collect();
    let mut name = String::new();
    let mut chars = link.name.chars().peekable();
    while let Some(c) = chars.next() {
        // Escape what could end the name early or start a qualifier, so the
        // link reads back the same wherever it appears.
        let next = chars.peek().copied();
        let special = match c {
            '\\' | '[' | ']' => true,
            '#' => next.is_some_and(|n| n == '[' || n.is_alphanumeric() || n == '_'),
            _ => false,
        };
        if special {
            name.push('\\');
        }
        name.push(c);
    }
    format!("[[{name}{qualifiers}]]")
}

fn format_in(value: &Value, ctx: Context) -> String {
    match value {
        Value::Text(s) => format_text(s, ctx),
        Value::Link(link) => format_link(link),
        Value::BlockRef(id) => format!("(({id}))"),
        Value::List(items) => {
            let items: Vec<_> = items
                .iter()
                .map(|v| format_in(v, Context::ListItem))
                .collect();
            let items = items.join(", ");
            // `[[[` would read as a link, so separate a leading `[`.
            let gap = if items.starts_with('[') { " " } else { "" };
            format!("[{gap}{items}]")
        }
    }
}

/// Writes text as it is if it would parse back unchanged, otherwise quoted.
///
/// Inside `(…)` or `[…]`, an unquoted `[[` or `((` could pair up with a
/// `]]` or `))` beyond the value, so text containing them is always quoted
/// there: whether it reads back can't be judged from the value alone.
fn format_text(s: &str, ctx: Context) -> String {
    let nested = !matches!(ctx, Context::Line);
    let as_is = !s.is_empty()
        && s.trim() == s
        && !s.contains(['\n', '\r'])
        && !(nested && (s.contains("[[") || s.contains("((")))
        && reparse(s, ctx).is_some_and(|v| v == Value::Text(s.to_owned()));
    if as_is {
        return s.to_owned();
    }
    let mut quoted = String::from('"');
    for c in s.chars() {
        match c {
            '"' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

fn reparse(raw: &str, ctx: Context) -> Option<Value> {
    match ctx {
        Context::Paren => single_inline_value(&format!("#k({raw})")),
        Context::ListItem => match single_inline_value(&format!("#k([{raw}])")) {
            Some(Value::List(items)) => match <[Value; 1]>::try_from(items) {
                Ok([item]) => Some(item),
                Err(_) => None,
            },
            _ => None,
        },
        Context::Line => Some(parse_line_value(raw)),
    }
}

fn single_inline_value(src: &str) -> Option<Value> {
    match parse_block(src).properties.as_slice() {
        // No need to check the span: the caller compares the value with the
        // whole of what was wrapped, so a property that ended early differs.
        [p] => p.value.clone(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Value {
        Value::Text(s.to_owned())
    }

    // Round trips are property-tested in tests/round_trip.rs; these pin down
    // that the output stays readable rather than defensively quoted.
    #[test]
    fn readable_where_possible() {
        assert_eq!(
            format_inline_property("status", Some(&text("done"))),
            "#status(done)"
        );
        assert_eq!(format_inline_property("due date", None), "#[[due date]]");
        assert_eq!(
            format_property_line("note", Some(&text("a, b"))),
            "note:: a, b"
        );
        assert_eq!(
            format_inline_property("note", Some(&text("a, b"))),
            "#note(a, b)"
        );
        assert_eq!(
            format_inline_property("tags", Some(&Value::List(vec![text("a, b"), text("c")]))),
            "#tags([\"a, b\", c])"
        );
        let jane = Value::Link(Link {
            name: "Jane".into(),
            qualifiers: vec![],
        });
        assert_eq!(
            format_inline_property("with", Some(&Value::List(vec![jane]))),
            "#with([ [[Jane]]])",
            "a space keeps `[[[` from reading as a link"
        );
        assert_eq!(
            format_link(&Link {
                name: "C# notes".into(),
                qualifiers: vec![]
            }),
            "[[C# notes]]"
        );
    }

    #[test]
    fn meta_lines_are_written_plainly() {
        assert_eq!(format_meta_line("transitive", None), "##transitive");
        assert_eq!(
            format_meta_line("values", Some("#type(a)")),
            "##values(#type(a))"
        );
        assert_eq!(format_meta_line("due date", Some("x")), "##[[due date]](x)");
    }

    /// Found by fuzzing: a parameter can end in a backslash if a space came
    /// before the `)`, and must be written with it.
    #[test]
    fn meta_parameter_ending_in_backslash() {
        let read = crate::parse_meta_line(r"##k(a\ )");
        assert_eq!(read, Some(("k".to_owned(), Some(r"a\".to_owned()))));
        let written = format_meta_line("k", Some(r"a\"));
        assert_eq!(crate::parse_meta_line(&written), read, "{written}");
    }

    /// Text that reads back alone can still pair with brackets after it:
    /// `#p(a [[b] c]) see x]]` would read as a link reaching past the `)`.
    #[test]
    fn values_stop_at_their_end() {
        for (value, after) in [("a [[b] c]", "x]]"), ("a ((b) c)", "x))")] {
            for written in [
                format_inline_property("p", Some(&text(value))),
                format_inline_property("p", Some(&Value::List(vec![text(value)]))),
            ] {
                let src = format!("{written} {after}");
                let block = crate::parse_block(&src);
                assert_eq!(block.properties.len(), 1, "{src}");
                assert_eq!(block.properties[0].span, 0..written.len(), "{src}");
            }
        }
    }
}
