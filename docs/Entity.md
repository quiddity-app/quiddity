alias:: [page, properties block]

- An entity is what other tools call a page: a root [[Block]] whose text is its name, with the page's body as its children.
- Properties block
	- If the first child is nothing but property lines, it is the entity's properties block, and holds the entity's own [[Properties]].
	- This mirrors the properties at the top of a [[Logseq]] page.
	- ```
	  Jane                         ← root block: the name
	  ├─ type:: person             ← properties block
	  │  born:: 1970-01-01
	  ├─ Met at [[Summit]]         ← the body
	  │  └─ …
	  ```
- Names are not unique.
	- An entity is identified by its name together with its properties, and a link can say which properties it means: `[[Jane#type(person)]]`. [[Links]]
	- Renaming keeps the old name as an [[alias]], so links written against it still work. [[Renaming]]
	- A [[Journal]] is identified by its date and a property entity by its name. Two copies made on different devices are merged. [[Duplicates]]
- Where entities come from
	- Writing a `[[link]]` to one that doesn't exist yet, and leaving the block, makes it. So does following such a link. [[Links to nowhere]]
	- Today's journal is made when the app starts.
	- Importing Markdown makes one per file. [[Export and import]]
- Every entity is listed in [[All entities]], where they can be deleted, and drawn in the [[Graph view]].
- Favourites: an entity with `favourite:: true` is listed on the left. [[favourite]]
