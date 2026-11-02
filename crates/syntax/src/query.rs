//! The query language: `#type(person) & ~#archived`.
//!
//! Operators and precedence are jj's revsets': prefix `~` binds tightest,
//! then `&` and infix `~` (and not), left to right, then `|`.

use std::fmt;

use crate::format::{format_key, format_link};
use crate::parse::{Link, Scanner, is_escapable, parse_key, parse_link, parse_quoted};

/// A parsed query. What it finds is up to whoever evaluates it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    /// `Name` or `"Name"`: entities with that name or alias, or, as a
    /// property's value, text equal to it.
    Name(String),
    /// `[[Name#key(value)]]`, resolved as a link is.
    Link(Link),
    /// `#key`, or `#key(query)` matched against each value of `key`.
    Has {
        key: String,
        value: Option<Box<Query>>,
    },
    /// `~a`
    Not(Box<Query>),
    /// `a & b`
    And(Box<Query>, Box<Query>),
    /// `a ~ b`
    Diff(Box<Query>, Box<Query>),
    /// `a | b`
    Or(Box<Query>, Box<Query>),
}

impl Query {
    /// The links written in the query, including those in values, in order.
    /// Links inside a link's qualifiers aren't counted, as in properties.
    pub fn links(&self) -> Vec<&Link> {
        let mut out = Vec::new();
        self.collect_links(&mut out);
        out
    }

    fn collect_links<'a>(&'a self, out: &mut Vec<&'a Link>) {
        match self {
            Query::Name(_) | Query::Has { value: None, .. } => {}
            Query::Link(link) => out.push(link),
            Query::Has { value: Some(v), .. } | Query::Not(v) => v.collect_links(out),
            Query::And(a, b) | Query::Diff(a, b) | Query::Or(a, b) => {
                a.collect_links(out);
                b.collect_links(out);
            }
        }
    }

    /// How tightly it binds: an operand of looser binding needs parentheses.
    fn level(&self) -> u8 {
        match self {
            Query::Or(..) => 1,
            Query::And(..) | Query::Diff(..) => 2,
            _ => 3,
        }
    }
}

/// Why a query couldn't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryError {
    /// Byte offset in the query's text.
    pub at: usize,
    pub message: &'static str,
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}", self.message, self.at)
    }
}

impl std::error::Error for QueryError {}

/// Parses a query. Unlike block text, a query can be malformed: whatever
/// holds it flags that rather than guessing.
pub fn parse_query(src: &str) -> Result<Query, QueryError> {
    let mut p = Parser {
        sc: Scanner::new(src),
    };
    let query = p.union()?;
    p.sc.skip_ws();
    if p.sc.at_end() {
        Ok(query)
    } else {
        Err(p.error("expected an operator"))
    }
}

/// Writes a query so that [`parse_query`] reads it back the same, with no
/// more parentheses or quotes than that needs.
pub fn format_query(query: &Query) -> String {
    write(query, 0)
}

fn write(q: &Query, min_level: u8) -> String {
    let s = match q {
        Query::Name(name) => format_name(name),
        Query::Link(link) => format_link(link),
        Query::Has { key, value: None } => format!("#{}", format_key(key)),
        Query::Has {
            key,
            value: Some(v),
        } => format!("#{}({})", format_key(key), write(v, 0)),
        Query::Not(a) => format!("~{}", write(a, 3)),
        Query::And(a, b) => format!("{} & {}", write(a, 2), write(b, 3)),
        Query::Diff(a, b) => format!("{} ~ {}", write(a, 2), write(b, 3)),
        Query::Or(a, b) => format!("{} | {}", write(a, 1), write(b, 2)),
    };
    if q.level() < min_level {
        format!("({s})")
    } else {
        s
    }
}

/// A name bare if it reads back as itself, otherwise quoted. A trailing
/// backslash would escape whatever follows the name, such as a `)`, so it
/// can't be judged alone and is always quoted.
fn format_name(name: &str) -> String {
    let bare = !name.contains(['\n', '\r'])
        && !name.ends_with('\\')
        && parse_query(name).is_ok_and(|q| q == Query::Name(name.into()));
    if bare {
        return name.to_owned();
    }
    let mut quoted = String::from('"');
    for c in name.chars() {
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

/// Characters that end a bare name.
fn ends_name(c: char) -> bool {
    matches!(c, '&' | '|' | '~' | '(' | ')' | '#' | '[')
}

/// Escapable in a query: the usual set, and the operators.
pub(crate) fn query_escapable(c: char) -> bool {
    is_escapable(c) || matches!(c, '&' | '|' | '~')
}

struct Parser<'a> {
    sc: Scanner<'a>,
}

impl Parser<'_> {
    fn error(&self, message: &'static str) -> QueryError {
        QueryError {
            at: self.sc.pos,
            message,
        }
    }

    fn eat_op(&mut self, op: &str) -> bool {
        self.sc.skip_ws();
        self.sc.eat(op)
    }

    fn union(&mut self) -> Result<Query, QueryError> {
        let mut q = self.intersection()?;
        while self.eat_op("|") {
            q = Query::Or(Box::new(q), Box::new(self.intersection()?));
        }
        Ok(q)
    }

    fn intersection(&mut self) -> Result<Query, QueryError> {
        let mut q = self.unary()?;
        loop {
            if self.eat_op("&") {
                q = Query::And(Box::new(q), Box::new(self.unary()?));
            } else if self.eat_op("~") {
                q = Query::Diff(Box::new(q), Box::new(self.unary()?));
            } else {
                return Ok(q);
            }
        }
    }

    fn unary(&mut self) -> Result<Query, QueryError> {
        if self.eat_op("~") {
            Ok(Query::Not(Box::new(self.unary()?)))
        } else {
            self.atom()
        }
    }

    fn atom(&mut self) -> Result<Query, QueryError> {
        self.sc.skip_ws();
        match self.sc.peek() {
            Some('(') => {
                self.sc.bump();
                let q = self.union()?;
                self.close()?;
                Ok(q)
            }
            Some('#') => {
                self.sc.bump();
                let key = parse_key(&mut self.sc).ok_or_else(|| self.error("expected a key"))?;
                if !self.sc.eat("(") {
                    return Ok(Query::Has { key, value: None });
                }
                let value = self.union()?;
                self.close()?;
                Ok(Query::Has {
                    key,
                    value: Some(Box::new(value)),
                })
            }
            Some('"') => parse_quoted(&mut self.sc)
                .map(Query::Name)
                .ok_or_else(|| self.error("unclosed quote")),
            Some('[') => parse_link(&mut self.sc)
                .map(Query::Link)
                .ok_or_else(|| self.error("expected a link")),
            _ => self.name(),
        }
    }

    fn close(&mut self) -> Result<(), QueryError> {
        if self.eat_op(")") {
            Ok(())
        } else {
            Err(self.error("expected `)`"))
        }
    }

    fn name(&mut self) -> Result<Query, QueryError> {
        let mut name = String::new();
        while let Some(c) = self.sc.peek() {
            if c == '\\'
                && let Some(e) = self.sc.rest()[1..]
                    .chars()
                    .next()
                    .filter(|&e| query_escapable(e))
            {
                self.sc.bump();
                self.sc.bump();
                name.push(e);
                continue;
            }
            if ends_name(c) {
                break;
            }
            name.push(c);
            self.sc.bump();
        }
        let name = name.trim();
        if name.is_empty() {
            Err(self.error("expected a query"))
        } else {
            Ok(Query::Name(name.to_owned()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(s: &str) -> Query {
        Query::Name(s.to_owned())
    }

    fn has(key: &str, value: Option<Query>) -> Query {
        Query::Has {
            key: key.to_owned(),
            value: value.map(Box::new),
        }
    }

    fn b(q: Query) -> Box<Query> {
        Box::new(q)
    }

    // Round trips are property-tested in tests/round_trip.rs; these pin down
    // what the parser makes of text people write.
    #[test]
    fn precedence() {
        assert_eq!(
            parse_query("a | b & ~c"),
            Ok(Query::Or(
                b(name("a")),
                b(Query::And(b(name("b")), b(Query::Not(b(name("c"))))))
            ))
        );
        assert_eq!(
            parse_query("a ~ b & c"),
            Ok(Query::And(
                b(Query::Diff(b(name("a")), b(name("b")))),
                b(name("c"))
            )),
            "`&` and infix `~` are one level, left to right"
        );
        assert_eq!(
            parse_query("(a | b) & c"),
            Ok(Query::And(
                b(Query::Or(b(name("a")), b(name("b")))),
                b(name("c"))
            ))
        );
        assert_eq!(
            parse_query("~~a"),
            Ok(Query::Not(b(Query::Not(b(name("a"))))))
        );
    }

    #[test]
    fn terms() {
        assert_eq!(
            parse_query("#parent(#type(country))"),
            Ok(has("parent", Some(has("type", Some(name("country"))))))
        );
        assert_eq!(parse_query(" #todo "), Ok(has("todo", None)));
        assert_eq!(
            parse_query("#[[due date]](soon)"),
            Ok(has("due date", Some(name("soon"))))
        );
        assert_eq!(parse_query("New  York "), Ok(name("New  York")));
        assert_eq!(parse_query(r#""Fish & chips""#), Ok(name("Fish & chips")));
        assert_eq!(parse_query(r"Fish \& chips"), Ok(name("Fish & chips")));
        assert_eq!(parse_query(r"C:\Users"), Ok(name(r"C:\Users")));
        assert_eq!(parse_query("O\"Brien"), Ok(name("O\"Brien")));
        let jane = parse_query("[[Jane#type(person)]]").unwrap();
        assert!(matches!(&jane, Query::Link(l) if l.name == "Jane" && l.qualifiers.len() == 1));
        assert_eq!(jane.links().len(), 1);
    }

    #[test]
    fn malformed() {
        for (src, at) in [
            ("", 0),
            ("a &", 3),
            ("(a", 2),
            ("#k()", 3),
            ("a)", 1),
            ("# x", 1),
            ("\"open", 0),
            ("[[a", 0),
            ("a [b]", 2),
            ("#todo.", 5),
        ] {
            assert_eq!(parse_query(src).map_err(|e| e.at), Err(at), "{src:?}");
        }
        assert_eq!(
            parse_query("a)").unwrap_err().to_string(),
            "expected an operator at 1"
        );
    }

    #[test]
    fn readable_where_possible() {
        for src in [
            "#type(person) & ~#archived",
            "a | b & ~c",
            "(a | b) & c",
            "a ~ (b ~ c)",
            "#parent(New York)",
            "\"Fish & chips\"",
            "[[Jane#type(person)]]",
            r#"#path("C:\\")"#,
        ] {
            assert_eq!(format_query(&parse_query(src).unwrap()), src);
        }
        assert_eq!(
            format_query(&name("a\nb\rc")),
            r#""a\nb\rc""#,
            "a query is one line, so it fits on a meta-property's"
        );
    }
}
