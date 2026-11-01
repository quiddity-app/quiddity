type:: crate

- What a UI shows of a graph, and the edits it makes to it: entities, blocks, references, journals, search, and the outliner's edits. It returns plain data.
- `crates/view`. It knows nothing of JavaScript or Tauri, so all of it is tested natively, in Rust, with the code the browser runs. [[quiddity-wasm]] [[Testing]]
- It also lists the devices the graph records (`View::devices`), most recently seen first. They are sync metadata kept beside the blocks, not entities. [[Device list]]
- Offsets are UTF-16 code units, which is what JavaScript and [[CodeMirror]] count in. Rust's byte offsets never leave the crate.
- Pages that lean on it: [[Editing]], [[Undo]], [[Links to nowhere]], [[All entities]], [[Graph view]], [[Title matching]] and [[Text search]].
