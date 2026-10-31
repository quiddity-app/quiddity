type:: library

- A CRDT library, in Rust: documents that several replicas edit apart and merge without conflicts. https://loro.dev
- Each [[Graph]] is one Loro document. Blocks are a `LoroTree`, so concurrent moves can't make loops, and each block's text is a `LoroText`, merging character by character. [[quiddity-core]]
- Quiddity uses its version vectors and updates for [[Sync]], shallow snapshots for [[Compaction]], `UndoManager` for [[Undo]], and its commit times for [[All entities]].
- Its surprises are written down in `crates/core/AGENTS.md`: imports that wait silently for their causes, compacted snapshots ignored after any import, and a deleted block restored under a new ID.
- Back to [[Library]].
