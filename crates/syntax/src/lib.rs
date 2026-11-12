//! Block-level Markdown syntax for Quiddity.
//!
//! A block's text is Markdown, and it is the source of truth for the block's
//! properties. This crate finds the Quiddity-specific constructs in that text:
//!
//! | Syntax                         | Meaning                                        |
//! |--------------------------------|------------------------------------------------|
//! | `key:: value`                  | a property, on a line of its own (Logseq style) |
//! | `#tag`                         | a tag: a property with no value                |
//! | `#[[multi word]]`              | a tag whose name contains spaces               |
//! | `[[Name]]`                     | a link to an entity by name                    |
//! | `[[Name#type(person)]]`        | a link narrowed by the target's properties     |
//! | `((id))`                       | a reference to a block by its `id` property    |
//! | `##key(parameter)`             | a `##` property, on a line of its own          |
//! | `##key:: value`                | the same, with a value as a property line has  |
//!
//! It also parses [`Query`]s, the language meta-properties' parameters are
//! written in, which, unlike block text, can be malformed.
//!
//! Parsing never fails: anything that isn't well-formed is plain text.

mod format;
mod name;
mod parse;
mod query;

pub use format::{
    format_key, format_link, format_meta_line, format_meta_value_line, format_property_line,
    format_qualifier, format_tag, format_value,
};
pub use name::{fold, is_word_char, nfc, normalise_name};
pub use parse::{
    Block, Link, MetaProperty, PropertiesBlock, Property, PropertyStyle, Qualifier, Value,
    is_properties_block, parse_block, parse_line_value, parse_meta_line, parse_meta_value_line,
    parse_properties_block, parse_property_line,
};
pub use query::{Query, QueryError, format_query, parse_query};
