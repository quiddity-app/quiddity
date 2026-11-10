//! Block IDs: a block with `##id(abc)` is the one `((abc))` refers to.
//!
//! The ID is written in the block's own text, so that it survives export and
//! import, which a block's internal ID doesn't. It goes in any block, unlike
//! other `##` lines, since a block is what it names.

use quiddity_syntax::{MetaProperty, normalise_name};

/// The `##` property that gives a block an ID to be referred to by.
pub const KEY: &str = "id";

/// Whether a `##` line is a `##id` line.
pub fn is_id(meta: &MetaProperty) -> bool {
    normalise_name(&meta.key) == KEY
}

/// The ID of the first `##id` line that has one. A line with none says
/// nothing, so it doesn't hide one after it: a block can be given an ID by
/// adding a line, whatever else it holds.
pub fn of(meta: &[MetaProperty]) -> Option<&str> {
    meta.iter()
        .filter(|m| is_id(m))
        .find_map(|m| m.parameter.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(text: &str) -> Vec<MetaProperty> {
        quiddity_syntax::parse_block(text).meta
    }

    #[test]
    fn the_first_id_line_is_the_id() {
        assert_eq!(of(&meta("words\n##id(abc-1)")), Some("abc-1"));
        assert_eq!(of(&meta("##ID( abc )\n##id(def)")), Some("abc"));
        for not in ["", "##id", "##id()", "id:: abc", "##id:: abc", "##ids(abc)"] {
            assert_eq!(of(&meta(not)), None, "{not:?}");
        }
        assert_eq!(
            of(&meta("##id\n##id(def)")),
            Some("def"),
            "a line with no ID doesn't hide one after it"
        );
    }
}
