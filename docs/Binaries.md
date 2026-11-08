- Files kept in a graph: images, PDFs and the like. They can be added, shown, moved between devices, exported and deleted when nothing refers to them; what is left is listed under Not built yet. [[Design]]
- Decided
	- A binary is an entity in the graph, an asset, and its properties block describes it with one `##asset` line. Nothing about it is stored outside its text, as for everything else. [[Meta-properties]] [[Design]]
	- The bytes live in an [[iroh-blobs]] store, not in the [[Loro]] document, and travel on iroh-blobs' own protocol. [[iroh]]
	- Every device keeps every blob. Options per device, to fetch on demand or to cap the cache on a phone, can come later.
	- The UI gets bytes through a [[Tauri]] command and shows them from object URLs. The published site has no store, so it shows a placeholder.
	- An export writes blobs to `assets/`, named `<blake3>.<ext>`, and the Markdown refers to them by that path. [[Export and import]]
- Garbage collection, by `quiddity-node gc`
	- A file is deleted only when all of these hold: the graph refers to it nowhere now; it has gone unreferenced for the grace period (30 days, as long as a device may be silent before it stops holding back [[Compaction]]); and it was first seen unreferenced at a version of the graph that every active device has since stored.
	- The last is what makes it safe. A device that has not heard that a file was dropped may be about to refer to it again; either it has stored a version from after the drop, and so knew, or it is not yet counted as having, and the file waits. A device silent for 30 days stops being waited for, as it does for compaction.
	- Nothing is deleted when the rule can't be applied: this replica lacks something another device has stored (there is no compaction point), the graph has never joined one (a blank replica), or the record of sightings can't be read, which only starts the clocks again.
	- A `##asset` line that can't be read whole still protects the file whose hash it begins with. A damaged line is a reason to look, not to delete.
	- The first run only notes what is unreferenced, in `unreferenced-blobs.txt` beside the snapshot; later runs delete what has come due. `--dry-run` says what would happen and changes nothing, not even that file. `--grace-days` changes the 30.
	- The node must not be serving: the store's database allows one process, which is also why nothing can be fetched while it runs. It is not run automatically, and the app does not run it.
	- iroh-blobs deletes only by its own garbage collection, which removes whatever is not marked live. So the node opens the store with a collector told to keep everything stored except the files due, lets go of the store's tags, and checks afterwards that every file it meant to keep is still there. The empty file is built into the store and never listed.
- Built
	- `Blobs` in [[quiddity-node]] keeps bytes by hash in `blobs/` beside the graph's snapshot: `put`, `get` and `has`. `get` says a blob is missing rather than failing in the store, and the store checks bytes against their hash as it reads them. [[Storage]]
	- `##asset(hash, mime, size, name)` is read by [[quiddity-core]]. The hash is the BLAKE3 hash in hex, the MIME type is `type/subtype`, the size is in bytes, and the name, the file name, may be left off. It is whatever follows the third comma, so it may contain commas; parentheses, brackets, quotes, `#` and backslashes in it are escaped with a backslash, as in any meta-property. [[Meta-properties]]
	- ```
	  cat.png                                       ← the asset's entity
	  └─ type:: image
	     ##asset(ea8f…200f, image/png, 12345, cat.png)
	  ```
	- An `##asset` line that can't be read, or a second one on the same entity, is flagged on its chip and the entity is not counted as an asset. The hash is not checked against the store: whether the bytes are here is a separate question. Nothing else is stored about an asset, so a graph written before this existed is unchanged.
	- A device that is told of an `##asset` it lacks fetches the bytes from a connected device, and the headless node keeps them all. [[Blob transfer]]
	- The app's shell keeps a store, and has `put_blob` and `get_blob` for the UI, so it gives and fetches blobs like the headless node does. [[The shell]]
	- [[quiddity-view]] says what file an entity page describes, and makes the entity for a file the shell has kept, so the UI never reads or writes the `##asset` line. It is exposed to the UI by [[quiddity-wasm]].
	- The UI: drop or paste a file on a page and it is kept, given an entity and linked; an asset's page shows an image from an object URL, or a download link, or says the bytes aren't here yet. [[Add a file]] [[The UI]]
	- Export writes the files to `assets/` and import keeps those named for their own hash. [[Export and import]]
- Not built yet
	- Showing a file inline where a page links to it, rather than only on its own page. A page links to a file by name, as it does to any entity; embedding one as a block reference ([[Block references]]) is how [[Logseq]] does it and is still to be done.
	- The app's download link has been tried in a browser and not in its webview, and a file dropped in the real window may need the webview told to hand it over.
	- Collecting files from the app, or without stopping a serving node.
	- Fetching on demand, and a limit on how much a phone keeps. Streaming a large file rather than reading it whole.
