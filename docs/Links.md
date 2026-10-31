alias:: [link, qualifier, backlinks, references]

- `[[Name]]` links to the [[Entity]] called Name. Names are compared ignoring case and repeated whitespace.
- Qualifiers
	- Names are not unique, so a link can say which properties its target has: `[[Jane#type(person)#born(1970-01-01)]]`. [[Properties]]
	- A qualifier `#key(value)` matches an entity with an instance of `key` whose value is equal to `value`, or contains it, for a list. A bare `#key` matches an entity with any instance of `key`.
	- A link shows only its name. The qualifiers pick the target, and appear when you hover over it. [[Rendering]]
	- Inside a link, `#` followed by a key starts a qualifier. Write `\#` for a literal one; `[[C# notes]]` is fine as it is.
- Writing a link: `[` pairs, and the page's title is completed as you type, with the qualifiers added when the title is shared. [[Editing]]
- Links are resolved when they are followed, not when they are written.
	- Exactly one match: it goes there. Click opens it in the middle, shift-click at the side. [[The UI]]
	- No match: the link makes the entity, with the properties its qualifiers name. [[Links to nowhere]]
	- Several matches: the link is ambiguous, and its tooltip says which qualifiers several entities match. A page to choose between them and narrow the link in place is planned, not built.
	- A renamed entity keeps its old name as an [[alias]], so links to the old name still resolve. [[Renaming]]
- References
	- The foot of every page lists the blocks that link to it, grouped by the entity they are in, journals first and newest first.
	- Unlinked references, found on request, are mentions of the entity's name or an alias as a whole word in prose: not inside a link, not in a properties block, and not in a journal's own date. Finding them means reading every block.
- Property values can be links too: `author:: [[Jane]]`, or bare `type:: person` for a property whose values are references. [[type]]
- Block references, `((id))`, point at a block rather than an entity. [[Block references]]
- Back to the [[User guide]].
