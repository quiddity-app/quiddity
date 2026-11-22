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
- The UI's main thread
	- The replica runs in the page's thread, in WASM, so every query, search and edit holds up what the person does next for as long as it takes. That is the design: nothing typed waits on IPC. [[The UI]]
	- What was done, so that nothing is made to wait that needn't be
		- **Saving can't be starved, and views are told less often.** Pushing to the shell was a debounce that restarted on every keystroke, so typing that never paused for 300 ms was not saved until it stopped. Telling views of typing was a throttle, every 120 ms, which read the page again and drew it eight times a second while someone typed, though the page the typing is in doesn't need it. `debounce` in `lib/debounce.ts` waits for quiet but never longer than a maximum from the first unanswered call: a second for both. Typing that never pauses is saved and shown every second, and any other typing a moment after it stops. The undo step is not capped: a burst of typing is one step however long it goes on, as in an editor.
		- **A read made again shares what didn't change** (`share` in `lib/share.ts`), so `RenderedBlock`'s `memo` can hold. Every edit clears what the graph has read, and every read makes new objects, so the memo had never once hit: every block of a page was split and drawn as Markdown again after each edit.
		- **Leaving a block that was not changed is not an edit** (`commitBlock`): it cleared everything read, told every view and marked the push as structure, which the shell must not hold back, for the sake of nothing.
		- **The sidebar and the references are read after the urgent render** (`useDeferredRead`). Recents asked for every entity's title to look up a few, and the references scan every block, on every edit.
		- **Search, the All entities filter and link completion** do their work after the keystroke has been drawn. [[Search]] [[All entities]] [[Editing]]
	- Measured in headless Chromium with the CPU slowed four times, on the graph above (3,150 pages, 38,000 blocks, as a snapshot) with a page of 300 blocks, from the key event's own time to the next paint, so that time spent waiting behind other work counts. Medians, before and after
		- | | before | after |
		  |---|---|---|
		  | Enter (a new block) | 32 to 42 ms | 31 to 39 ms |
		  | Tab (indent) | 42 to 43 ms | 36 to 39 ms |
		  | Shift-Tab (outdent) | 42 to 48 ms | 38 to 39 ms |
		  | long tasks over 50 ms, in total | 3.8 to 4.1 s | 3.3 to 3.5 s |
	- **That is a small change, and the interaction was not slow to begin with** in this setup: about 8 to 12 ms a key on a real CPU. It did not show the stall that makes adding, indenting and outdenting feel slow in the app, so the cause there is not found. The first query after a structural edit rebuilds the index, 219 ms native in the `scale` example above (see "Not done"), which the browser run did not reach; the real shell, a real graph and a webview other than Chromium are all different. Measure there before changing more.
	- Not done, in the order it would pay
		- **Search and completion in a worker**, which has its own replica fed with the updates the page makes, so that a search of 3,150 pages (120 ms of text search) never holds up a keystroke. A facet may already return a promise, and link completion already takes its answer after the key, so neither changes shape. The cost is a second copy of the graph in memory and an answer a moment behind.
		- **The graph view's layout in a worker.** `d3-force` ticks in the page, and restarts at full heat when entities arrive.
		- **The whole replica in a worker** would need every read to be asynchronous, which `useSyncExternalStore` and the kept reads don't allow. Not planned.
- Code: `Graph::scan`, `scan_reusing` and `Index::from_scan` in `quiddity-core`, `Corpus` and `Change` in `crates/view/src/lib.rs`.
