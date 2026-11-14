type:: crate

- What a UI shows of a graph, and the edits it makes to it: entities, blocks, references, journals, search, and the outliner's edits. It returns plain data.
- `crates/view`. It knows nothing of JavaScript or Tauri, so all of it is tested natively, in Rust, with the code the browser runs. [[quiddity-wasm]] [[Testing]]
- It also lists the devices the graph records (`View::devices`), most recently seen first. They are sync metadata kept beside the blocks, not entities. [[Device list]]
- Offsets are UTF-16 code units, which is what JavaScript and [[CodeMirror]] count in. Rust's byte offsets never leave the crate.
- It turns what [[Built-in properties]] say into what a page shows: chips for `##` lines, flags in words on the chips they concern, inherited values, and inverse trees.
- Pages that lean on it: [[Editing]], [[Undo]], [[Links to nowhere]], [[All entities]], [[Graph view]], [[Title matching]], [[Text search]] and [[Graph settings]].
- A block says what file it is, if it is a blob (`Block.blob`: hash, MIME type, size and name), so the UI never reads the `##blob` line itself. `attach_blob` adds a block for a file the shell has kept at the end of a page, checking its parts first, so a bad answer leaves nothing behind, as one undo step. [[Binaries]]
