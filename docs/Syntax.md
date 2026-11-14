- Everything Quiddity reads from a [[Block]]'s text, in one place. The text is Markdown, and these are read from it.
- At a glance
	- | Written | Meaning |
	  |---|---|
	  | `key:: value` | a property, on a line of its own |
	  | `#key` | a tag: a property with no value |
	  | `#[[Multi word]]` | a tag whose name has spaces |
	  | `[[Name]]` | a link to the entity called Name |
	  | `[[Name#type(person)]]` | a link narrowed by its target's properties |
	  | `author:: [[Jane#type(person)]]` | a link as a property's value |
	  | `((id))` | a reference to the block with that `##id(…)` line |
	  | `[text]([[Name]])` | a link to an entity, with words of its own |
	  | `[text](((id)))` | a link to a block |
	  | `[text](/graph/)`, `[text](https://…)` | a link to an app page, or the web |
	  | `##key(parameter)` | a meta-property, on a line of its own in a property's own properties block |
	  | `##id(abc)` | gives its block an ID, which `((abc))` refers to |
	  | `##key:: value` | the same, with a value written as a property line's is |
	- [[Properties]], [[Links]], [[Block references]] and [[Built-in properties]] have the details, and [[Queries]] has the language a meta-property's parameter is written in.
- Values
	- Text, written bare, or `"quoted"` when it contains a delimiter.
	- A link, `[[Name#key(value)]]`, or a block reference, `((id))`.
	- A list of those, `[a, "b, c", [[D]]]`. Lists are always written with brackets, and don't nest.
	- What a value means beyond that, a number or a date say, is up to the property's schema. [[Properties]]
- Escapes
	- `\` before any of `\ ( ) [ ] , " #` makes it literal.
	- Any other backslash is literal: `path:: C:\Users` reads as written.
	- Quoted text also understands `\n` for a new line.
- When `#` starts a tag
	- At the start of a line, after whitespace or `(`, or straight after another tag, so `#a#b` is two. A `(value)` after a tag is ordinary text: `#key(value)` is the tag `key` followed by "(value)". Only a link's qualifiers take values.
	- `# Heading`, `foo#bar` and a URL's fragment are not tags.
	- A key of nothing but digits is not one either, so issue numbers like `#123` stay as text. `#fff` is a tag; we can live with that.
	- A full stop at the end isn't part of a bare key: `see #todo.` is `todo`.
	- Inside a link, `#` followed by a key starts a qualifier. Write `\#` for a literal one; `[[C# notes]]` is fine as it is.
- Property lines
	- A key starts with a letter, digit or underscore, may contain spaces, and may not contain `:`, `#`, backticks, brackets or parentheses, so prose that mentions a `key:: value` code span stays prose.
	- A line of nothing but `#key` is the same as a property line with no value, and is how one is written.
	- In a property line a comma means nothing special: `note:: a, b` is the text "a, b".
- Meta-property lines
	- `##key:: value`, alone on a line, has a value written as a property line's: a link or a list needs no query syntax. `::` must be followed by a space or the end of the line, and a line with no value is `##key`. It has no parameter, and a line with a parameter has no value.
	- `##key` or `##key(parameter)`, alone on a line. The parameter is kept as written and is usually a query ([[Queries]]); its parentheses balance, so it needs no escaping.
	- A meta-property line is not prose: a `#key(value)` in its parameter is part of the query, not a tag of the block.
- Code spans and fenced code are opaque: nothing in them is a property, a link or a reference.
- Anything malformed is plain text. Reading never fails. The parser is [[quiddity-syntax]], and the UI gets its answers through [[quiddity-view]], so there is only one.
