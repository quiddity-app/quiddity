alias:: text format

- A graph exports to a directory of Markdown that is readable on its own and imports again with nothing lost. It can also import a [[Logseq]] graph.
- From the command line, with [[quiddity-node]]
	- `quiddity-node -g ~/graph export ~/export` writes the graph to `~/export`.
	- `quiddity-node -g ~/graph import ~/export` reads a directory into the graph. A blank replica can't import: it has to join by syncing first. [[Sync]]
	- `quiddity-node serve --export ~/export` exports the *running* graph when the process gets SIGUSR1 (`kill -USR1 <pid>`), replacing `~/export`. It is written beside it as `~/export.new` and swapped in, so a reader never sees a half-written export. The export is done when the node prints `exported N entities to …`; the signal itself says nothing, so a hook should wait for that line (or for `~/export` to appear afresh) before the backup proceeds. Unix only.
	- The app has no export or import of its own yet.
- The directory
	- ```
	  export/
	    quiddity.json        marks a Quiddity export, with its format version
	    quiddity.toml        the graph's settings, if any are set
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
	- A properties block that has children, or blank lines at either end of its text, is written as the first bullet instead, since the header is trimmed of blank lines when read.
	- Child bullets are indented with one tab per level. Continuation lines get the bullet's tabs and two spaces.
	- A block's own properties are part of its text, so they need no special handling.
- Importing
	- With `quiddity.json` present, indentation is read strictly, and the round trip is exact. It is tested that way.
	- Without it, as for a Logseq graph or these pages, indentation is judged by width, fenced code is respected, and `logseq/` and hidden directories are skipped.
	- A `quiddity.toml` at the top sets the graph's settings, with or without `quiddity.json`. [[Graph settings]]
	- Files in `journals/` named `2026-10-25.md` or `2026_10_25.md` become journals. [[Journal]]
	- A file with no `title::` is named for the file. In a Logseq graph, `___` in the file name is a `/`, and a name with a `/` is a namespace: the page gets a `parent` to the page above it. [[Logseq]]
	- Importing the same export twice makes two of everything except journals and property entities, which merge. Exports leave out copies that have been merged. [[Duplicates]]
