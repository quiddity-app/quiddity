- Files kept in a graph: images, PDFs and the like. They can be added, shown, moved between devices, exported and deleted when nothing refers to them; what is left is listed under Not built yet. [[Design]]
- Decided
	- A binary is a block, a blob: its first line is `##blob(hash, mime, size, name)` and any text after it is its caption. It is not an entity and has no page of its own: it sits in the page it was added to. Nothing about it is stored outside its text, as for everything else. [[Meta-properties]] [[Design]]
	- The bytes live in an [[iroh-blobs]] store, not in the [[Loro]] document, and travel on iroh-blobs' own protocol. [[iroh]]
	- Every device keeps every blob. Options per device, to fetch on demand or to cap the cache on a phone, can come later.
	- The UI gets bytes through a [[Tauri]] command and shows them from object URLs. The published site has no store, so it shows a placeholder.
	- An export writes blobs to `assets/`, named `<blake3>.<ext>`, and the Markdown refers to them by that path. [[Export and import]]
- Garbage collection, by `quiddity-node gc`
	- A file is deleted only when all of these hold: the graph refers to it nowhere now; it has gone unreferenced for the grace period (30 days, as long as a device may be silent before it stops holding back [[Compaction]]); and it was first seen unreferenced at a version of the graph that every active device has since stored.
	- The last is what makes it safe. A device that has not heard that a file was dropped may be about to refer to it again; either it has stored a version from after the drop, and so knew, or it is not yet counted as having, and the file waits. A device silent for 30 days stops being waited for, as it does for compaction.
	- Nothing is deleted when the rule can't be applied: this replica lacks something another device has stored (there is no compaction point), the graph has never joined one (a blank replica), or the record of sightings can't be read, which only starts the clocks again.
	- A `##blob` line that can't be read whole still protects the file whose hash it begins with. A damaged line is a reason to look, not to delete.
	- The first run only notes what is unreferenced, in `unreferenced-blobs.txt` beside the snapshot; later runs delete what has come due. `--dry-run` says what would happen and changes nothing, not even that file. `--grace-days` changes the 30.
	- The node must not be serving: `gc` saves the graph it loaded, which would be written over what a serving node saved since. Nothing enforces this: the old store's lock did, by accident, and the blobs now being in the graph's file there is no lock to do it. It is not run automatically, and the app does not run it.
	- Deleting is one transaction in the graph's file that removes exactly the files named. (iroh-blobs offers a client no way to delete, only its own collector, so the node's store does it itself.) The empty file is built in and never listed.
- Built
	- `Blobs` in [[quiddity-node]] keeps bytes by hash in the graph's file: `put`, `get`, `has` and `delete`. A graph's old `blobs/` directory is moved into the file the first time it is opened, and set aside as `blobs.migrated/`. `get` says a blob is missing rather than failing in the store, and the store checks bytes against their hash as it reads them. [[Storage]]
	- `##blob(hash, mime, size, name)` is read by [[quiddity-core]], from the first line of a block. The hash is the BLAKE3 hash in hex, the MIME type is `type/subtype`, the size is in bytes, and the name, the file name, may be left off. It is whatever follows the third comma, so it may contain commas; parentheses, brackets, quotes, `#` and backslashes in it are escaped with a backslash, as in any meta-property. [[Meta-properties]]
	- ```
	  Holiday                                       ← a page
	  ├─ Day one
	  └─ ##blob(ea8f…200f, image/png, 12345, cat.png)   ← a blob
	     The cat, asleep                               ← its caption
	  ```
	- A block is a blob only if its first line is a `##blob` line, so a blob can be the first block of a page without being taken for its properties block. A `##blob` line that can't be read, or is not the first line, is flagged on its chip and the block is not a blob. The hash is not checked against the store: whether the bytes are here is a separate question. Nothing else is stored about a blob.
	- The same bytes in two blocks are two blobs and one stored file. A blob can be given an `##id(…)` line and embedded elsewhere with `((id))`; the embedding is not built. [[Block references]]
	- A device that is told of a `##blob` it lacks fetches the bytes from a connected device, and the headless node keeps them all. [[Blob transfer]]
	- The app's shell keeps a store, and has `put_blob` and `get_blob` for the UI, so it gives and fetches blobs like the headless node does. [[The shell]]
	- [[quiddity-view]] says what file a block is, and adds a block for a file the shell has kept, so the UI never reads or writes the `##blob` line. It is exposed to the UI by [[quiddity-wasm]].
	- The UI: drop or paste a file on a page and it is kept and added to the page as a blob; the block shows an image from an object URL, or a download link, or says the bytes aren't here yet, with its caption below. [[Add a file]] [[The UI]]
	- Export writes the files to `assets/` and import keeps those named for their own hash. [[Export and import]]
- Not built yet
	- Embedding a blob somewhere other than where it is, by a block reference ([[Block references]]), as [[Logseq]] does.
	- The app's download link has been tried in a browser and not in its webview, and a file dropped in the real window may need the webview told to hand it over.
	- Collecting files from the app, or without stopping a serving node.
	- Fetching on demand, and a limit on how much a phone keeps. Streaming a large file rather than reading it whole.
