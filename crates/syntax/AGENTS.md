# quiddity-syntax

The reference parser and formatter for block text: `key:: value` lines,
`#tag`s, `[[links#with(qualifiers)]]`, `((block refs))` and `##` lines.

- **The formatter checks itself by reparsing.** `format_text` writes text
  bare only if the parser reads it back unchanged; otherwise it quotes. So a
  parser bug usually just makes the formatter quote more, and round-trip
  tests still pass. Parser behaviour needs its own examples
  (`parse.rs` tests), and readable output its own (`readable_where_possible`).
- **Nested values quote `[[` and `((`** even when they read back alone:
  `[[x#p(a [[b] c])]] see x]]` would otherwise read as a link reaching past the
  `)`. Whether a value is safe can't be judged from the value alone.
- **Lists start `[ ` when the first item starts with `[`**, so `[[[x]]]`
  isn't read as a link.
- **Link escaping is context-free**: `\`, `[` and `]` always, and `#`
  before a key character or `[`. A link reads back the same wherever it
  appears.
- **Names compare as `normalise_name` makes them**: NFKC, case folded, no
  zero-width space, word joiner or byte order mark. Storage is NFC (`nfc`,
  applied by `quiddity-core` when text is written). Don't compare names with
  `to_lowercase`. `is_word_char` (letters, digits and combining marks) is shared
  by tag keys and the search tokeniser.
- **Keys and link names are single lines.** They're titles, written with
  whitespace collapsed (`normalise_name`); the generators exclude `\n` and
  `\r` for that reason.
- **References come from structure.** Links and block refs in a value count
  at the top level or in a list, but not inside quoted text or qualifiers.
  `Refs` collects them for line values and `##key:: value` lines, whose text it reads again when the value isn't exactly a list, quote, link or reference.
- **A tag has no value.** `#key(value)` is the tag `key` and then the text
  `(value)`, so what follows a tag can't change what the tag is (the `block`
  fuzz target's "each property can be parsed from its span alone"). Only a
  link's qualifiers take values, and they are read by `parse_paren_value`.
  The formatter proves a qualifier value reads back by wrapping it in a link,
  `[[x#k(…)]]`, which is the only place one is ever written.
- **`#key` alone on a line is `key::`**, for now. `parse_property_line` (and so
  `is_properties_block`) accepts it, and `parse_block` reads it as a tag.
  `format_property_line` writes a property with no value as `key::`.
- `#` starts a tag only after whitespace, `(`, the start of
  the text, or directly after another tag (`#a#b`). Code
  spans and fences are skipped; all-digit keys (`#123`) aren't keys.
- **Meta-property lines and queries must agree on where a query ends.**
  `parse_meta_line` finds the `)` that closes the parameter by skipping
  escapes (the query set, with `& | ~`), `#key`s, quoted names at the start
  of a term, and links, the way `parse_query` reads them, with only space
  and tab as whitespace. Change one and change the other: the `query` fuzz
  target and `meta_lines_and_queries_agree` check it.
- **A trailing backslash escapes what follows.** `format_query` never ends a
  bare name with one, and `format_meta_line` puts a space before the `)`
  after a parameter that ends with one.
- **`##key:: value` is a meta-property line too** (`parse_meta_value_line`,
  `MetaProperty::value`). A line is one form or the other, never both: a
  parameter is kept as text for a query, a value is read as a property line's
  is.
- **A meta-property line isn't prose**: it isn't scanned for tags. Links in its parameter count if the parameter is a query.
- **Queries can be malformed**, unlike block text: `parse_query` returns
  where it went wrong, for whatever holds the query to flag.

Robustness against arbitrary text is the `block` and `query` fuzz targets'
job; the proptests check round trips of generated values.
