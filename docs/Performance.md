- What costs time in a large graph, what has been done about it, and what hasn't. Measured with `cargo run --release -p quiddity-view --example scale`, which builds a made-up graph and times each whole-graph answer. [[quiddity-view]]
- The case that prompted it
	- A [[Logseq]] import of 3,150 pages (mostly journals) has a 3.3 MB `graph.loro`. The example's default is the same size: 3,150 pages of 11 blocks, 38,000 blocks, a 3.7 MB snapshot.
	- The app took a while to start, and every pause in typing was followed by a stall.
- Where the time went
	- Not in parsing: reading the text of every block is 23 ms. It is in asking [[Loro]] for blocks one at a time, which costs about 7 µs a block however it is asked: 200 ms to walk the tree with `children`, and 330 ms to read every block's text.
	- Each whole-graph answer did that again for itself: the index, `summaries`, and every `search_text`. An edit made the index stale, and the next query read the whole graph again to rebuild it.
- What was done
	- `Graph::scan` reads the tree's structure in one call (`get_nodes`, 8 ms instead of 200), and `scan_reusing` takes the text of blocks it already has instead of asking again. [[quiddity-core]]
	- `View` keeps what it read (the corpus) between revisions. Typing says which block it changed, so only that block's text is read again; creating or moving blocks changes no text; anything else (an undo, an import) reads everything. A property test checks that a view edited for a while agrees with one made fresh from the same graph. [[quiddity-view]]
	- The index is built from the corpus, and records which entity each block is in, so `summaries`, `connections` and `search_text` stop asking Loro for each block's root. Titles come from the index, not from Loro.
	- The All entities table draws 200 rows and more as the end is reached, as the journal feed does. [[All entities]]
- Measured at that size, in milliseconds
	- | | native before | native after | WASM before | WASM after |
	  |---|---|---|---|---|
	  | load and first index | 630 | 460 | 650 | 570 |
	  | first read after typing a character | 560 | 80 | 470 | 110 |
	  | first read after an undo or import | 560 | 330 | | |
	  | `summaries()` | 620 | 180 | 670 | 400 |
	  | `search_text()` | 450 | 60 | 560 | 120 |
	  | `entities()` | 34 | 1 | 38 | 12 |
	  | `connections()` | 50 | 12 | 61 | 50 |
	- Native is the `scale` example. WASM is the same graph (`scale 3150 5 <path>` saves it) loaded into the generated `quiddity_wasm` in Node, which is about one and a half to two times slower than native, with the cost of handing results to JavaScript added. Browsers differ; run it in the one you use.
	- **`summaries()` and `connections()` are the least improved in the browser**, because much of what remains is turning thousands of results into JavaScript objects, which a Rust change doesn't touch. Only the All entities and graph pages ask for them.
- Not done, in the order it would pay
	- **An incremental index.** Typing still rebuilds all of it, and 80 ms is mostly that: parsing every block again (35 ms), resolving every link (17 ms) and working out effective properties (20 ms). Parsing could be kept per block; links and effective properties need to know which entities an edit touches.
	- **Starting without reading everything.** The first index is the cost of reading every block from Loro once, and that is about 330 ms of the 460. Only keeping something derived on disk avoids it. [[Storage]] plans that, in SQLite.
	- **`summaries()`** still walks every block for its time of last move (77 ms of the 180) and for its words and tags. It could be kept per entity and redone for the entities an edit touched.
	- **An inverted index for full-text search**, kept beside the index. `search_text` now tokenises every block per query, 60 ms, which is fine for a search box and not for anything more.
	- **Recents** asks for every entity's title to look up a few. A call that answers for given IDs would stop it serialising 3,150 objects across WASM after each edit.
	- **The undo history** has not been looked at. [[Undo]]
- Code: `Graph::scan`, `scan_reusing` and `Index::from_scan` in `quiddity-core`, `Corpus` and `Change` in `crates/view/src/lib.rs`.
