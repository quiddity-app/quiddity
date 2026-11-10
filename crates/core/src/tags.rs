//! Tags: `#person #employee` in a properties block and `##tag:: [person,
//! employee]` say the same thing, and both are read into the same set.
//!
//! A tag is a link to the entity it names that is read as classification. The
//! index holds one as a property with no value, written inline, which is what
//! it was before `##tag` existed: queries, link qualifiers and inheritance
//! already treat a bare `#key` that way.

use quiddity_syntax::{Block, Link, MetaProperty, Property, PropertyStyle, Value, normalise_name};

/// The `##` property that holds a list of tags.
pub const KEY: &str = "tag";

/// Whether a `##` line is a `##tag` line.
pub fn is_tag(meta: &MetaProperty) -> bool {
    normalise_name(&meta.key) == KEY
}

/// The names a `##tag` value holds, as written. A value that names nothing, a
/// reference to a block, say, holds none.
pub fn names(value: &Value) -> Vec<(&str, bool)> {
    fn collect<'a>(value: &'a Value, out: &mut Vec<(&'a str, bool)>) {
        match value {
            Value::Text(t) if !t.trim().is_empty() => out.push((t.trim(), false)),
            Value::Link(link) => out.push((link.name.as_str(), true)),
            Value::List(items) => items.iter().for_each(|v| collect(v, out)),
            Value::Text(_) | Value::BlockRef(_) => {}
        }
    }
    let mut out = Vec::new();
    collect(value, &mut out);
    out
}

/// The link a tag is: to the entity of that name.
pub fn link(key: &str) -> Link {
    Link {
        name: key.to_owned(),
        qualifiers: Vec::new(),
    }
}

/// Adds each `##tag` line's values to `block`'s properties as tags, in the
/// order they are written, and returns the links they add that `block` doesn't
/// already have: a value that is a link is one of `block.links` already.
pub fn expand(block: &mut Block) -> Vec<Link> {
    let mut added = Vec::new();
    for meta in block.meta.iter().filter(|m| is_tag(m)) {
        let Some(value) = &meta.value else { continue };
        for (name, is_link) in names(value) {
            block.properties.push(Property {
                key: name.to_owned(),
                value: None,
                style: PropertyStyle::Inline,
                span: meta.span.clone(),
            });
            if !is_link {
                added.push(link(name));
            }
        }
    }
    block.properties.sort_by_key(|p| p.span.start);
    added
}

#[cfg(test)]
mod tests {
    use super::*;
    use quiddity_syntax::parse_properties_block;

    fn read(text: &str) -> (Vec<(String, usize)>, Vec<String>) {
        let mut block = parse_properties_block(text).unwrap().block;
        let before = block.links.len();
        let added = expand(&mut block);
        assert_eq!(block.links.len(), before, "links already read aren't added");
        (
            block
                .properties
                .iter()
                .map(|p| (p.key.clone(), p.span.start))
                .collect(),
            added.into_iter().map(|l| l.name).collect(),
        )
    }

    #[test]
    fn the_long_form_is_the_same_as_the_short() {
        let (long, _) = read("##tag:: [person, employee]");
        let (short, _) = read("#person #employee");
        let keys = |v: &[(String, usize)]| v.iter().map(|p| p.0.clone()).collect::<Vec<_>>();
        assert_eq!(keys(&long), keys(&short));
    }

    #[test]
    fn values_are_names_whatever_they_are_written_as() {
        let (props, added) = read("##tag:: [a, [[B c]], \"d e\", ((ref)), \"\"]");
        let keys: Vec<_> = props.iter().map(|p| p.0.as_str()).collect();
        assert_eq!(keys, ["a", "B c", "d e"]);
        assert_eq!(added, ["a", "d e"], "a link is already a link");
        assert_eq!(read("##tag:: single").0[0].0, "single");
        assert!(read("##tag::\nkind:: a").1.is_empty(), "no value, no tags");
    }

    #[test]
    fn tags_stay_in_the_order_they_are_written() {
        let (props, _) = read("#first\nkind:: x\n##tag:: [second]\n#third");
        let keys: Vec<_> = props.iter().map(|p| p.0.as_str()).collect();
        assert_eq!(keys, ["first", "kind", "second", "third"]);
    }

    #[test]
    fn other_lines_are_not_tag_lines() {
        let (props, added) = read("##tags:: [a]\n##tag(b)\n##Tag:: [c]");
        let keys: Vec<_> = props.iter().map(|p| p.0.as_str()).collect();
        assert_eq!(keys, ["c"], "the key is matched as names are");
        assert_eq!(added, ["c"]);
    }
}
