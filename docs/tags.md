type:: property

- A tag is a property with no value, `#tag`, or a value of a `tags` property, written as a list: `tags:: [a, b]`. Without the brackets, `tags:: a, b` is one tag, "a, b". [[Properties]]
- Tags are compared as names are, ignoring case and spacing.
- [[All entities]] shows enough tags to tell apart entities with the same title.
- Nothing defined tags before the table needed them, so this is a reading, not a settled part of the model. `tags_in` in `crates/view/src/query.rs` is the one place to change it.
