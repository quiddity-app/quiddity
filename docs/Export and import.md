alias:: text format

- A graph exports to a directory of Markdown that is readable on its own and imports again with nothing lost. It can also import a [[Logseq]] graph.
- From the command line, with [[quiddity-node]]
	- `quiddity-node -g ~/graph export ~/export` writes the graph to `~/export`.
	- `quiddity-node -g ~/graph import ~/export` reads a directory into the graph. A blank replica can't import: it has to join by syncing first. [[Sync]]
	- The app has no export or import of its own yet.
- The directory
	- ```
	  export/
	    quiddity.json        marks a Quiddity export, with its format version
	    entities/
	      Jane.md
	      Jane 2.md          same name, different entity: only the file name differs
	    journals/
	      2026-10-25.md
	  ```
	- [[Binaries]] will go in `assets/`, once there are any.
- A file is one [[Entity]], as an outline in the style of Logseq's original format.
	- ```markdown
	  title:: Jane
	  type:: person
	  born:: 1970-01-01

	  - Met at [[Platform Summit#type(event)]] #status(done)
	    a second line of the same block
	  	- A child block, with a ((0198f0c4-…)) reference
	  - Quotable
	    id:: 0198f0c4-…
	  ```
	- The lines before the first bullet are the entity's name, as `title::`, followed by its properties block. So names needn't be valid or unique file names. [[title]]
	- Child bullets are indented with one tab per level. Continuation lines get the bullet's tabs and two spaces.
	- A block's own properties are part of its text, so they need no special handling.
- Importing
	- With `quiddity.json` present, indentation is read strictly, and the round trip is exact. It is tested that way.
	- Without it, as for a Logseq graph or these pages, indentation is judged by width, fenced code is respected, and `logseq/` and hidden directories are skipped.
	- Files in `journals/` named `2026-10-25.md` or `2026_10_25.md` become journals. [[Journal]]
	- A file with no `title::` is named for the file.
	- Importing the same export twice makes two of everything except journals and property entities, which merge. Exports leave out copies that have been merged. [[Duplicates]]
- Back to the [[User guide]].
