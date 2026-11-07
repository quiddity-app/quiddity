type:: crate

- What a UI shows of a graph, and the edits it makes to it: entities, blocks, references, journals, search, and the outliner's edits. It returns plain data.
- `crates/view`. It knows nothing of JavaScript or Tauri, so all of it is tested natively, in Rust, with the code the browser runs. [[quiddity-wasm]] [[Testing]]
- It also lists the devices the graph records (`View::devices`), most recently seen first. They are sync metadata kept beside the blocks, not entities. [[Device list]]
- Offsets are UTF-16 code units, which is what JavaScript and [[CodeMirror]] count in. Rust's byte offsets never leave the crate.
- It turns what [[Meta-properties]] say into what a page shows: chips for `##` lines, flags in words on the chips they concern, inherited values, and inverse trees.
- Pages that lean on it: [[Editing]], [[Undo]], [[Links to nowhere]], [[All entities]], [[Graph view]], [[Title matching]], [[Text search]] and [[Graph settings]].
- An entity page says what file it describes, if it is an asset (`Entity.asset`: hash, MIME type, size and name), so the UI never reads the `##asset` line itself. `create_asset` makes the entity for a file the shell has kept, checking its parts first, so a bad answer leaves nothing behind, and `attach_asset` does that and ends a page with a link to it, written by the syntax crate so it is escaped, as one undo step. [[Binaries]]
