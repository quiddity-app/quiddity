alias:: text format

- A graph exports to a directory of Markdown that is readable on its own and imports again with nothing lost. It can also import a [[Logseq]] graph.
- From the command line, with [[quiddity-node]]
	- `quiddity-node -g ~/graph export ~/export` writes the graph to `~/export`, and says how many files it wrote to `assets/`. A file the graph names but this device doesn't have yet is counted as not on this device, and left out.
	- `quiddity-node -g ~/graph import ~/export` reads a directory into the graph. A blank replica can't import: it has to join by syncing first. [[Sync]]
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
	    assets/
	      ea8f163d…200f.png  a file, named for the BLAKE3 hash of its bytes
	  ```
	- [[Binaries]] go in `assets/` as `<hash>.<extension>`, one file for each hash. The extension is the file name's own if it has a short plain one, otherwise the type's, otherwise `bin`; it is for file managers and browsers, and the hash is what identifies the file. So a text export can refer to a file by its path, and a tool that reads Markdown can follow it. The page for the file says what it is in its `##asset` line, which is in its Markdown like anything else. [[Add a file]]
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
	- A `quiddity.toml` at the top sets the graph's settings, with or without `quiddity.json`. [[Graph settings]]
	- Files in `assets/` are kept if they are named for the hash of what is in them, with any extension or none. Others are left alone and counted: a [[Logseq]] graph's `assets/` is full of pictures that nothing here refers to by hash, and a file that doesn't match its name is damaged or not ours. Importing them again changes nothing.
	- Files in `journals/` named `2026-10-25.md` or `2026_10_25.md` become journals. [[Journal]]
	- A file with no `title::` is named for the file.
	- Importing the same export twice makes two of everything except journals and property entities, which merge. Exports leave out copies that have been merged. [[Duplicates]]
