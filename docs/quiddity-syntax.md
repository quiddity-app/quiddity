type:: crate

- The reference parser and formatter for block text: `key:: value` lines, inline `#key(value)` properties, `[[links#with(qualifiers)]]` and `((block refs))`. [[Syntax]]
- `crates/syntax`. Everything else that reads block text goes through it, the UI included, by way of [[quiddity-view]] in WASM.
- Parsing never fails: anything malformed is plain text. The formatter checks itself by reading back what it wrote.
- Property-tested for round trips, and fuzzed (the `block` target) for arbitrary text. [[Testing]]
