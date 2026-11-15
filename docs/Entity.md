alias:: [page, properties block]

- An entity is what other tools call a page: a root [[Block]] whose text is its name, with the page's body as its children.
- Properties block
	- If the first child starts with a property line, a line of tags or a `##` line, it is the entity's properties block, and holds the entity's own [[Properties]]. Its later lines must be of those kinds too: any other line is read as nothing, and flagged under the entity's title. A first child that starts with prose is an ordinary block.
	- The properties block is drawn on its own ground (a tint, so it reads as a different kind of block), and `Block::properties_block` tells the UI which it is. An entity that has none offers "+ Add properties", which adds an empty first block to type into, and takes it away again if it is left empty, since an empty first block means nothing. [[Editing]]
	- Typing in it completes. At the start of a line it offers the keys the graph already uses, most used first, with the property entities that have `##` lines even if nothing uses them yet. After `key::` it offers the values already given to that key. Enter chooses one. Only a properties block completes (and the block the offer made), never prose. [[Properties]]
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
	- A [[Journal]] is identified by its date. Two copies made on different devices are merged. [[Duplicates]]
- Where entities come from
	- Writing a `[[link]]` to one that doesn't exist yet, and leaving the block, makes it. So does following such a link. [[Links to nowhere]]
	- Today's journal is made when the app starts.
	- Importing Markdown makes one per file. [[Export and import]]
- Every entity is listed in [[All entities]], where they can be deleted, and drawn in the [[Graph view]].
- Favourites: an entity with `favourite:: true` is listed on the left. [[favourite]]
