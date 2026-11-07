- Files kept in a graph: images, PDFs and the like. The store, the way the graph describes a binary and the transfer between devices are built; the rest is planned. [[Design]]
- Decided
	- A binary is an entity in the graph, an asset, and its properties block describes it with one `##asset` line. Nothing about it is stored outside its text, as for everything else. [[Meta-properties]] [[Design]]
	- The bytes live in an [[iroh-blobs]] store, not in the [[Loro]] document, and travel on iroh-blobs' own protocol. [[iroh]]
	- Every device keeps every blob. Options per device, to fetch on demand or to cap the cache on a phone, can come later.
	- The UI gets bytes through a [[Tauri]] command and shows them from object URLs. The published site has no store, so it shows a placeholder.
	- An export writes blobs to `assets/`, named `<blake3>.<ext>`, and the Markdown refers to them by that path. [[Export and import]]
	- Garbage collection: a blob nothing refers to is deleted after a grace period, and only once every device's stored version is past the point where the last reference went. [[Compaction]]
- Built
	- `Blobs` in [[quiddity-node]] keeps bytes by hash in `blobs/` beside the graph's snapshot: `put`, `get` and `has`. `get` says a blob is missing rather than failing in the store, and the store checks bytes against their hash as it reads them. Nothing calls it yet. [[Storage]]
	- `##asset(hash, mime, size, name)` is read by [[quiddity-core]]. The hash is the BLAKE3 hash in hex, the MIME type is `type/subtype`, the size is in bytes, and the name, the file name, may be left off. It is whatever follows the third comma, so it may contain commas; parentheses, brackets, quotes, `#` and backslashes in it are escaped with a backslash, as in any meta-property. [[Meta-properties]]
	- ```
	  cat.png                                       ← the asset's entity
	  └─ type:: image
	     ##asset(ea8f…200f, image/png, 12345, cat.png)
	  ```
	- An `##asset` line that can't be read, or a second one on the same entity, is flagged on its chip and the entity is not counted as an asset. The hash is not checked against the store: whether the bytes are here is a separate question. Nothing else is stored about an asset, so a graph written before this existed is unchanged.
	- A device that is told of an `##asset` it lacks fetches the bytes from a connected device, and the headless node keeps them all. [[Blob transfer]]
- To build, in this order
	- A shell command to put a file in the graph and one to read it back, and the UI: paste or drop a file, show images, link the rest.
	- Export and import.
	- Garbage collection.
- A block that refers to an asset is a block reference. [[Block references]]
