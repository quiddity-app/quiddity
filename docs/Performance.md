- What costs time in a large graph, what has been done about it, and what hasn't. Measured with `cargo run --release -p quiddity-view --example scale`, which builds a made-up graph and times each whole-graph answer. [[quiddity-view]]
- The case that prompted it
	- A [[Logseq]] import of 3,150 pages (mostly journals) has a 3.3 MB snapshot. The example's default is the same size: 3,150 pages of 11 blocks, 38,000 blocks, a 3.7 MB snapshot.
	- The app took a while to start, and every pause in typing was followed by a stall.
- Where the time went
	- Not in parsing: reading the text of every block is 23 ms. It is in asking [[Loro]] for blocks one at a time, which costs about 7 µs a block however it is asked: 200 ms to walk the tree with `children`, and 330 ms to read every block's text.
	- Each whole-graph answer did that again for itself: the index, `summaries`, and every `search_text`. An edit made the index stale, and the next query read the whole graph again to rebuild it.
- What was done
	- `Graph::scan` reads the tree's structure in one call (`get_nodes`, 8 ms instead of 200), and `scan_reusing` takes the text of blocks it already has instead of asking again. [[quiddity-core]]
	- `View` keeps what it read (the corpus) between revisions. Typing says which block it changed, so only that block's text is read again; creating or moving blocks changes no text; anything else (an undo, an import) reads everything. A property test checks that a view edited for a while agrees with one made fresh from the same graph. [[quiddity-view]]
	- The index is built from the corpus, and records which entity each block is in, so `summaries`, `connections` and `search_text` stop asking Loro for each block's root. Titles come from the index, not from Loro.
	- **The index is updated in place for typing in an ordinary block.** Only that block's backlinks can change, so `Index::update_text` takes them out and puts the new ones in, in document order, and the view patches that one block's text in the corpus without reading the structure. Typing in a root, a properties block or a block with an `##id` line builds the index again, because what they say is what the index is made of; so does anything that makes, moves or deletes blocks. `View::work` counts the scans and builds, which is how the tests (and the `scale` example) can tell. [[quiddity-core]]
	- **`summaries()` keeps what it works out.** Reading when the graph's history says each block was edited (`text_edits_since`) was 200 ms of every call, because it first read every block's text container to know which block a changed container was; Loro can say where a container is, so now only the changed ones are asked. What each block's text gives (words, tags) is kept until that text changes, and when each block was put where it is until blocks may have moved, so typing re-reads one block. `View::work` counts both.
	- The All entities table draws 200 rows and more as the end is reached, as the journal feed does. [[All entities]]
- Measured at that size, in milliseconds
	- | | native before | native after | WASM before | WASM after |
	  |---|---|---|---|---|
	  | load and first index | 630 | 460 | 650 | 570 |
	  | first read after typing a character | 560 | 80 | 470 | 110 |
	  | the same, in an ordinary block, with the index updated in place | | 2 | | 1 |
	  | the same, in a properties block or an entity's name | | 80 | | 80 |
	  | first read after an undo or import | 560 | 330 | | |
	  | first read after making, moving or deleting a block | | 100 | | |
	  | `summaries()`, the first time | 620 | 250 | 670 | 480 |
	  | `summaries()` after typing | 620 | 30 | 670 | 32 |
	  | `search_text()` | 450 | 60 | 560 | 120 |
	  | `entities()` | 34 | 1 | 38 | 12 |
	  | `connections()` | 50 | 12 | 61 | 50 |
	- Native is the `scale` example. WASM is the same graph (`scale 3150 5 <path>` saves it) loaded into the generated `quiddity_wasm` in Node, which is about one and a half to two times slower than native, with the cost of handing results to JavaScript added. Browsers differ; run it in the one you use.
	- **`summaries()` after typing is now mostly handing 3,150 rows to JavaScript**, which a Rust change doesn't touch, and `connections()` is much the same. Only the All entities and graph pages ask for them. The first `summaries()` of a session is the one that reads the whole history for when blocks were edited, once.
- Not done, in the order it would pay
	- **The rest of the index, incrementally.** Typing in a properties block or a name, and anything that makes or moves blocks (Enter, indent), still builds it again: about 100 ms. A properties block changes entities' properties, which feed inherited values and inverse labels, so doing it in place needs to know which entities those reach; the repeat-until-nothing-changes pass in `effective.rs` would have to start from the entities that changed.
	- **Starting without reading everything.** The first index is the cost of reading every block from Loro once, and that is about 330 ms of the 460. Only keeping something derived on disk avoids it. [[Storage]] plans that, in SQLite.
	- **`summaries()`** is still a pass over every block (about 30 ms) after an edit, and 175 ms after one that makes or moves blocks, which read when each block was put where it is again. Keeping totals per entity and redoing the entities an edit touched would take both away, at the price of keeping the incoming-links counts and each title's distinguishing tags right as links change.
	- **An inverted index for full-text search**, kept beside the index. `search_text` now tokenises every block per query, 60 ms, which is fine for a search box and not for anything more.
	- **Recents** asks for every entity's title to look up a few. A call that answers for given IDs would stop it serialising 3,150 objects across WASM after each edit.
	- **The undo history** has not been looked at. [[Undo]]
- Code: `Graph::scan`, `scan_reusing` and `Index::from_scan` in `quiddity-core`, `Corpus` and `Change` in `crates/view/src/lib.rs`.
