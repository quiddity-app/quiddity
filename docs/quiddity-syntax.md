type:: crate

- The reference parser and formatter for block text: `key:: value` lines, `#tag`s, `[[links#with(qualifiers)]]`, `((block refs))` and `##key(parameter)` and `##key:: value` lines, and for [[Queries]]. [[Syntax]] [[Meta-properties]]
- `crates/syntax`. Everything else that reads block text goes through it, the UI included, by way of [[quiddity-view]] in WASM.
- Parsing never fails: anything malformed is plain text. The formatter checks itself by reading back what it wrote.
- Property-tested for round trips, and fuzzed (the `block` and `query` targets) for arbitrary text. The `query` target also checks that a meta-property line and the query parser agree on where a query ends. [[Testing]]
