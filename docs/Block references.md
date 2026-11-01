alias:: block reference

- `((id))` refers to one [[Block]], wherever it is.
- Block IDs are internal to a [[Graph]] and don't survive export and import, so a reference goes through the block's `id::` property instead: a block with `id:: 0198f0c4-…` is the one `((0198f0c4-…))` means. [[id]]
- Built so far: the reference is parsed and resolved by the index, and shown as its text.
- Planned
	- Adding the `id::` line when a block is first referred to, as [[Logseq]] does.
	- Showing the block referred to in place, which is also how binaries will be embedded. [[Binaries]]
