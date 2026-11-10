type:: prior art

- An open-source outliner that keeps its pages as Markdown files. https://logseq.com
- Quiddity's closest relative, from which it takes: the outliner, journals as home, `key:: value` properties with a page's own in its first block, block references (written with a `##id(…)` line here, not `id::`), and the shape of the Markdown files. [[Properties]] [[Export and import]]
- A Logseq graph can be imported: its `journals/` become journals, and `logseq/` is skipped.
- Namespaces
	- Logseq makes `Places/Scotland` a child of `Places`, and writes the slash as `___` in the file name (`Places___Scotland.md`) unless the file has a `title::`. The import reads `___` back as `/`.
	- Each namespaced page gets a `parent` property naming the page above it, so `Places/Scotland/Edinburgh` has `parent:: [[Places/Scotland]]`. A namespace with no page of its own is made, as Logseq does. A page that already writes a `parent` keeps it.
	- If the graph doesn't define `parent`, the import does, as a property with `##inverse(children)`, so each page lists its children. [[Meta-properties]]
	- Only an import without `quiddity.json` does this: Quiddity's own exports keep a `/` in a name as it is.
