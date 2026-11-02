alias:: [query, query language]

- A query picks out entities by their properties: `#type(person) & ~#archived`. The first use is in [[Meta-properties]], to say which entities a property's values may refer to and which may have it. Queries in search and in blocks of their own are for later. Queries can't yet be written anywhere but a meta-property.
- At a glance
	- | Written | Finds |
	  |---|---|
	  | `#key` | entities with the property `key`, whatever its value |
	  | `#key(value)` | entities with a value of `key` that `value` matches |
	  | `Name`, `"Name"` or `[[Name]]` | entities called Name, or with it as an [[alias]] |
	  | `[[Name#type(person)]]` | the same, narrowed as a link is |
	  | `a & b` | entities found by both |
	  | `a \| b` | entities found by either |
	  | `a ~ b` | entities found by `a` and not `b` |
	  | `~a` | every entity `a` doesn't find |
	  | `(a)` | `a`, to group |
	- Prefix `~` binds tightest, then `&` and `~` between two queries, left to right, then `\|`. So `a | b & ~c` is `a | (b & (~c))`. The operators and their order are [[jj]]'s, from its revsets.
	- Spaces around operators don't matter.
- Names
	- A bare name runs until an operator, a parenthesis, `#` or `[`, and may have spaces in it: `#parent(New York)`. It is compared as names are, ignoring case and repeated whitespace.
	- A name containing any of those is quoted, `"Fish & chips"`, or linked, `[[Fish & chips]]`. Backslash escapes are as in the rest of the [[Syntax]], with `\&`, `\|` and `\~` as well.
	- A `"` starts a quoted name only at the start of a term. Elsewhere it is part of the name, so `O"Brien` needs no quoting.
- Values
	- In `#key(…)`, the parentheses hold another query, which is matched against each of an entity's values of `key` in turn.
	- A name matches a value that is the same text, compared as names are, or a reference to an entity with that name or alias. So `#rating(5)` and `#type(person)` both work.
	- Any other query matches a reference whose target it finds: `#parent(#type(country))` finds entities whose parent is a country.
	- `&`, `|` and `~` combine what each single value matches: `#rating(4 | 5)` finds a rating of 4 or of 5. So `#tag(a & b)` asks for one value that is both, which no value is; for an entity tagged with both, write `#tag(a) & #tag(b)`.
	- There are no comparisons yet, such as a rating above 3 or a date before today.
- What a query sees
	- An entity's effective properties: those it writes, those it inherits, and the values a transitive property reaches. With `##transitive` on [[type]], `#type(plant)` finds an oak whose type is tree, if tree's type is plant. [[Meta-properties]]
	- Inverse labels, as if they were properties: with `##inverse(children)` on `parent`, `#children(Edinburgh)` finds Edinburgh's parents.
- In a meta-property, a query is written as it is: `##values(#type(city) | #type(country))`. Its parentheses balance, so nothing needs escaping.
- Link qualifiers are queries of a sort already: `[[Jane#type(person)]]` is `Jane & #type(person)`. They are to share one evaluator, which would let a qualifier's value be a query too, as in `[[Jane#employer(#type(charity))]]`. [[Links]]
- Still to come: `all()` and `none()`, functions such as ancestors to a given depth, comparisons, and a query's matches shown in a block.
- How it works: [[quiddity-syntax]] parses a query into a tree, and writes it back out with no more quotes or parentheses than it needs. Unlike block text, a query can be malformed, and the parser says where. [[quiddity-core]] answers it against the index's effective properties. A link in a query finds every entity it matches, so `[[Jane]]` finds both Janes; a reference among a property's values counts only if it resolves to one entity.
