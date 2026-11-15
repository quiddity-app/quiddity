type:: prior art

- An open-source outliner that keeps its pages as Markdown files. https://logseq.com
- Quiddity's closest relative, from which it takes: the outliner, journals as home, `key:: value` properties with a page's own in its first block, block references (written with a `##id(…)` line here, not `id::`), and the shape of the Markdown files. [[Properties]] [[Export and import]]
- A Logseq graph can be imported: its `journals/` become journals, and `logseq/` is skipped.
	- Logseq links to a day by its own name for it, `[[10th November 2026]]` (its `do MMMM yyyy` format), where a [[Journal]] here is named `2026-11-10`. So that such links don't point nowhere, the import gives the journal that name as an `alias`, for every day that something links to that way, in a link, a tag or a property value. The ordinal has to be right (`11st` isn't a day) and the case doesn't matter. Other date formats are not recognised.
	- Logseq only makes a day's file once something is written on it, so a day that is linked to and has no file is made as a journal, with the alias. A day nothing links to by name gets no alias.
	- Only an import without `quiddity.json` does this.
- Namespaces
	- Logseq makes `Places/Scotland` a child of `Places`, and writes the slash as `___` in the file name (`Places___Scotland.md`) unless the file has a `title::`. The import reads `___` back as `/`.
	- Each namespaced page gets a `parent` property naming the page above it, so `Places/Scotland/Edinburgh` has `parent:: [[Places/Scotland]]`. A namespace with no page of its own is made, as Logseq does. A page that already writes a `parent` keeps it.
	- If the graph doesn't define `parent`, the import does, as a property with `##inverse(children)`, so each page lists its children. [[Built-in properties]]
	- Only an import without `quiddity.json` does this: Quiddity's own exports keep a `/` in a name as it is.
