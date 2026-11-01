- Everything Quiddity reads from a [[Block]]'s text, in one place. The text is Markdown, and these are read from it.
- At a glance
	- | Written | Meaning |
	  |---|---|
	  | `key:: value` | a property, on a line of its own |
	  | `#key` | a property with no value: a tag |
	  | `#key(value)` | a property, inline |
	  | `#key([a, b])` | one property holding a list |
	  | `#[[Multi word]](value)` | a property whose name has spaces |
	  | `[[Name]]` | a link to the entity called Name |
	  | `[[Name#type(person)]]` | a link narrowed by its target's properties |
	  | `#author([[Jane#type(person)]])` | a link as a property's value |
	  | `((id))` | a reference to the block with that `id::` |
	- [[Properties]], [[Links]] and [[Block references]] have the details.
- Values
	- Text, written bare, or `"quoted"` when it contains a delimiter.
	- A link, `[[Name#key(value)]]`, or a block reference, `((id))`.
	- A list of those, `[a, "b, c", [[D]]]`. Lists are always written with brackets, and don't nest.
	- What a value means beyond that, a number or a date say, is up to the property's schema. [[Properties]]
- Escapes
	- `\` before any of `\ ( ) [ ] , " #` makes it literal.
	- Any other backslash is literal: `path:: C:\Users` reads as written.
	- Quoted text also understands `\n` for a new line.
- When `#` starts a property
	- At the start of a line, after whitespace or `(`, or straight after another inline property, so `#a(1)#b(2)` is two.
	- `# Heading`, `foo#bar` and a URL's fragment are not properties.
	- A key of nothing but digits is not one either, so issue numbers like `#123` stay as text. `#fff` is a property; we can live with that.
	- A full stop at the end isn't part of a bare key: `see #todo.` is `todo`.
	- Inside a link, `#` followed by a key starts a qualifier. Write `\#` for a literal one; `[[C# notes]]` is fine as it is.
- Property lines
	- A key starts with a letter, digit or underscore, may contain spaces, and may not contain `:`, `#`, brackets or parentheses.
	- A line of nothing but `#key` is the same as a property line with no value, and is how one is written.
	- In a property line a comma means nothing special: `note:: a, b` is the text "a, b".
- Code spans and fenced code are opaque: nothing in them is a property, a link or a reference.
- Anything malformed is plain text. Reading never fails. The parser is [[quiddity-syntax]], and the UI gets its answers through [[quiddity-view]], so there is only one.
