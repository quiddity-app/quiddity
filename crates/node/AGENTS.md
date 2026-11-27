# quiddity-node

Sync over iroh (`Peer`), the on-disk `Store`, and the `quiddity-node` CLI.

- **A `Peer` relays.** Updates from one connection call `changed()`, which
  flushes every other connection, so the headless node passes edits between
  devices that are never online together.
- **`Peer::tidy` merges duplicates and tells connected devices.** `serve`
  calls it, then saves, shortly after every change (debounced 500 ms).
  Devices don't have to merge for themselves to get the result.
- **`serve` saves as changes arrive**, not only on Ctrl-C. The CLI test
  kills the server outright to check that.
- **`serve --export DIR` exports on SIGUSR1** (unix), from the live graph in
  memory, into `DIR.new` and then swapped over `DIR`. It prints
  `exported N entities to DIR` when done: the signal can't report that, so
  callers wait for the line. Without `--export` the signal is logged and
  ignored (the default action would kill the process).
- **`serve --debug-http ADDR` is `debug_http.rs`**: a hand-written HTTP/1.1
  responder (no HTTP dependency, one request per connection, `GET /` or
  `/status` only) over tokio's `TcpListener`. It is how anything learns about a
  running node, since the store allows one process at a time. It has no
  authentication and must stay free of graph content: state, device IDs and
  when they last saved, nothing a user wrote. The JSON is written by hand
  (`quote` escapes it); don't add a field that holds text from the graph.
- **The graph is one SQLite file, `graph.quiddity`**, with a `snapshot` row,
  an `update_log` of chunks that follow it, a `slot` row (a random salt) and, only in a graph made before the device's key
  was kept apart, an `endpoint_key` row (`store.rs`). `save` is one transaction in a rollback journal with `synchronous=FULL`, so it
  is atomic and durable. **It appends `updates_since(durable)` to the log
  and leaves the snapshot alone** (`Store::updates_to_append`), and writes a
  new snapshot, folding the log in, only when the log would pass half the
  snapshot's size, there is no snapshot, or `can_update(durable)` fails. A fold
  keeps the snapshot and log it replaces as generation 2, which `load` falls
  back to if generation 1 (snapshot or any chunk) can't be applied. **History is
  compacted only at a fold**: a lone device is its own compaction point, so
  compacting on every save would fold on every save. Tests that mean a fold call
  `write(graph, true)`; `save` decides by size, which tiny test graphs make
  unpredictable.
  Saves are serialised in-process by the `durable` mutex shared by clones, and
  across processes by the file's lock (`BUSY` bounds the wait). **The file
  is untrusted input**: `open` checks `application_id`, `user_version` and that
  the schema is *exactly* the `CREATE` statements in `TABLES`, so extend a table
  by changing both the statement and `LAYOUT`, with a conversion. Connections
  set defensive mode and turn triggers, views and trusted schema off; don't
  loosen those to make something convenient work. The file is made 0600 before
  SQLite touches it (the key is in it, and SQLite gives its journal the file's
  mode). Journal mode is `WAL`, so readers and the writer don't block each
  other. While a graph is open there are `-wal`/`-shm` beside it, so copy or
  move it only when it is closed (the last connection to close checkpoints them
  away). `set_aside` moves them with the file. The key is made inside a transaction, so two
  processes starting together agree on it.
- **`blob_store` is an iroh-blobs store on SQLite, and `Blobs` is a thin wrapper of it.**
  iroh-blobs' `Store` is a client of a command channel, and `FsStore` and
  `MemStore` are actors serving it. `blob_store::spawn` is a third, built from
  `Store::from_client`, which iroh-blobs documents for this,
  so **a new iroh-blobs release may still break it: pin the version and read its
  `store/mem.rs` first.** It is modelled on that file and tested against
  `MemStore` as an oracle (`tests/blob_store.rs`): the encoding of any part of
  any blob must match byte for byte. Things that aren't obvious:
  - **Deleting is `SqliteBlobs::delete`, not the protocol's.** iroh-blobs hides
    deletion from clients (only its own collector sends `DeleteBlobs`).
  - **Only whole blobs are rows, and only bytes are imported.** A fetch from
    another device is in memory, grown by verified data only (never by a size
    the peer claims, which is checked against the limit first), and written in
    one transaction when whole; a crash loses it and it is fetched again. A
    partial blob is served only in whole groups of 16 chunks, as iroh-blobs'
    own store does (`a_partial_blob_is_served_as_it_is_held`). Importing a
    stream or a path and exporting to a path are refused: the node reads and
    writes files itself.
  - **No tags are kept, temp tags protect nothing, and every batch is in
    `Scope::GLOBAL`** (a `Scope` can't be made outside iroh-blobs): nothing is
    deleted for being untagged, so nothing needs protecting. Asking for a tag
    works and listing tags finds none. Don't build on them. Keeping them cost a
    fifth of this file's mutants for nothing that reads them.
  - **The size limit is `open_limited`'s argument** (`MAX_BLOB`, SQLite's limit
    on one value, by default), so tests reach it without a gigabyte.
  - **A PR's mutants must finish within the CI job's timeout** (`ci.yml`),
    at about 14 seconds each. It was 20 minutes, roughly 60 mutants, when this
    store was written, which is why it came in two PRs, and why code that
    nothing reads (tags, other import paths) isn't written.
  - **`outboard` comes before `data` in the table**, so reading the small one
    doesn't walk the overflow pages of the large one. Data is read through
    SQLite's incremental blob I/O, under the connection's lock, in blocking
    code inside async tasks: fine at this size, not for many large concurrent
    exports.
  - **A write waits for the graph's own saves, and the other way round**, both
    being transactions on one file; `BUSY` bounds the wait.
  - **The test data must not repeat**: a short period made a read at the wrong
    offset return the right bytes, and a mutant survived because of it.
- **`legacy_blobs::adopt` moves an old `blobs/` directory into the file.** It
  reads the iroh-blobs filesystem store there (so `fs-store` stays a feature
  of `iroh-blobs` until this is dropped), keeps every blob, and only then
  renames the directory `blobs.migrated/`: a failure leaves it as it was, and a
  second try passes over what is already kept. It acts only on a directory that
  has `blobs.db` in it, so another `blobs/` is left alone, and a graph with no
  old store never has one made for it. `Blobs::open` calls it, so a graph's
  files come across the first time anything opens them.
- **The device's key is in a `Keyring`, not in any graph** (`keyring.rs`: one
  `device.key` of `qdk_` text in the configuration directory, made by a staged
  file and a hard link so that processes starting together agree on it, and
  never replaced if it can't be read). A `Store` is made with the keyring it
  uses, so say where it is: `--config-dir`/`QUIDDITY_CONFIG_DIR` on the CLI,
  the app's config directory in the shell, and a directory of the test's own
  in a test, never the real one. `Store::key` is the file's own `endpoint_key`
  if it has one (so an old graph's device ID is unchanged, and it never asks for
  the device key), and otherwise derived from the device key and the file's
  salt (`quiddity-identity`). **Layout 3** added `slot` (layout 2 was the `blob`
  table) and **layout 4** `node`, **layout 5** `unreferenced`; an earlier layout is
  converted on `open` by `upgrade`, which checks
  that the file is exactly that layout *under the write lock*, not before it:
  two processes opening one old file together used to see the other's tables
  and call it damaged. Extending the schema again is a new layout and a
  conversion.
- **A replica edits as a kept node ID.** `Store::load` and `init` set it
  (`Graph::set_node_id`; `init` makes the graph as that node, so nothing is
  written under a throwaway peer). It is stored sealed, with a mask derived from
  the device's key and the file's salt, so a file from another device doesn't
  open it and gets a new one. A *copy beside the original on one device* opens
  as the same node, so `Store::node_id` also asks the table of places
  (`places.rs`, `places.sqlite3` beside the key): the claim and the record are
  one write transaction, so copies opened together can't both be first. A graph
  seen at another path whose old place still has the node ID is a copy and forks
  (`fork_node_id`); one whose old place has gone, or lost it, was moved. Looking
  at the other place is `peek_node_id` on a read-only connection: **never use
  `open` there**, which converts an older layout, and the graph may be another
  process's. If the other can't be read it is *assumed to hold* the node. If the
  table can't be used the graph opens and a warning says so: a safeguard, not a
  precondition. Two processes that load one
  graph and both edit have always lost one's edits (each saves a whole
  snapshot); as one node they would also be a twin, so don't run a command
  beside `serve` that edits. `save` keeps the graph's node ID when the graph
  gave one up (`retired_node_ids`, `keep_node_id`) and only then: a graph handed
  to `save` that edits as some other node doesn't change what is stored.
- **`Store::new` takes the graph as it was named: a directory, or the file** (and the keyring).
  An existing directory is a directory, an existing file is the file whatever
  it is called (so a copy can be renamed), and a path that isn't there yet is a
  file if it ends in `.quiddity` and a directory if not, so every path that has
  named a directory still does. Pass `store.dir()`, the path as named, to
  whatever else opens the graph (`Blobs::open`, `gc::collect`): they make their
  own `Store::file_only`, which never asks for the device's key. **Anything that looks for files of an older layout (`graph.loro`,
  `device.key`, `blobs/`, the text ledger) must ask `legacy_dir()`**, which is
  `None` for a graph named by its file: its neighbours' files are not its own.
- **Never publish or copy a graph's file as a snapshot**: a graph made before the
  device's key was kept apart holds its endpoint key. `quiddity-node snapshot` writes the bare Loro snapshot for the
  static build, and a test checks the key isn't in it.
- **A directory of the old layout (`graph.loro`, `device.key`) is converted on
  first open** and the old files renamed `*.migrated`, not deleted. It is built
  in a staging file and renamed, so a crash leaves no half-made graph.
- **A claim is only ever about what is already durable.** `Store` keeps the
  version the file is known to hold (`durable`, set by `load` and after each
  successful write), and a save records *that* (`record_stored_at`), not the
  version it is about to write: peers compact on the record as soon as it
  arrives, so a failed write must not have run ahead of the file. The record
  lags one save. Compaction doesn't: it uses `compaction_point_as`, as if this
  device had already stored what the save writes, which is safe because the
  file will hold it. A one-shot CLI command therefore records what it loaded.
- **Saving records and compacts.** `Store::save` records this device (its
  endpoint ID) in the graph's device list, then writes a shallow snapshot
  at the compaction point (devices silent for `ACTIVE`, 30 days, don't
  count). The graph in memory keeps its history, so open sessions aren't
  affected; only what's on disk is compacted. A blank replica records
  nothing, and `import` refuses one: it must join by syncing first.
- **The graph's device list is the allow list.** `accept` lets in a device
  that is listed (checked on each connection) or in `Allowed::Only`.
  `Peer::allow` / `quiddity-node allow` list a device with nothing stored,
  which also stops compaction past what it might lack until it has synced;
  a blank replica refuses, since an edit with nothing beneath it can't be
  applied by a compacted device. A device that connects is listed by the
  session's vouching, so one explicit `--allow` is enough to start. Listing
  is one device's say-so for all of them: that is the model, but it means
  any member can add one. Nothing revokes yet.
- **`Peer::dial_listed` dials the listed devices** (`serve` runs it, except
  with `--local`). One task per device, spawned in a `JoinSet` so aborting
  the dialler stops them, and aborted individually (by the handle kept in
  `dialling`) when the device stops being listed or trusted: a removed device
  must not go on being dialled. `address` is called for every attempt: a device
  that came back elsewhere would otherwise be dialled at its old address.
  **Connecting has its own timeout** (10 s): QUIC waits far longer for a dead
  address, and that wait is the redial. Devices that both dial may hold two
  connections at once; it works but doubles the traffic.
- **`serve` and `save_on_change` are the library's, shared by the CLI and
  the app shell.** `serve` binds the endpoint, runs the router and (unless
  `local`) the dialler; dropping the `Server` stops the dialler, and
  `shutdown` closes the endpoint. **`Peer::trust` is for now only**: it lets a
  device connect without listing it, for a blank replica to let in the device
  it joins through; that device lists it on connecting. `Peer::admit` is the
  pairing action: it lists a device, or trusts it if this replica has not joined
  a graph yet. **`dial_listed` also dials the trusted devices** (it wakes on
  `trust`), because a blank replica lists no one: waiting to be dialled meant
  both sides had to be reachable and awake at the right time.
- **`Peer::events` is the network log** (`events.rs`, `docs/Network log.md`): a
  ring of the last 200 things the peer did, kept so the debug page can say why
  two devices don't sync (refused, another graph, couldn't connect, closed by
  the peer and why). `record` adds one and bumps the status watch, so the UI
  hears it; `connect` and `accept` log how a connection began and ended, `sync`
  logs both graph IDs when it finds another graph (only the side that reads the
  hello first has them: the other is told by the close code). **Keep it free of
  graph content**, as `debug_http` is: devices and reasons only.
- **`Peer::sync_details` is what the app's cloud shows** (`sync_state` is its `state`) (`docs/Sync status.md`).
  Each connection records what its peer is known to have (`Session::theirs`)
  in `Peer::syncs` with the device it is to, and the details are worked out when asked (`classify`, pure
  and tested on its own). **Behind is not "the versions differ"**: saving
  records this device in the graph without sending it, so they would differ
  for ever. It is *this device lacks something a peer has*, or *typing is held
  back*. `subscribe_status` is bumped when a link comes or goes, a peer's
  version moves, presence changes, and typing is held or sent.
- **`serve_at` is `serve` told where some devices are** (`known_addresses`
  reads `ID=ADDRESS,…`). With `local` there is no lookup, so a device can only
  be dialled where it is told; with any known address the listed devices are
  dialled even in local mode. The app's `QUIDDITY_PEERS` feeds it. It is how
  two instances on one machine find each other.
- **A device behind its peer's history** gets `Error::Behind`. The side
  that finds its peer behind closes with code `BEHIND`, so the other can say
  why even if the close arrives before the peer's hello is read. **Another
  graph** is the same: `Error::OtherGraph`, close code `OTHER_GRAPH`. Without
  the code the side that didn't notice first reads only "done", and a test that
  looks for the error is flaky. **Change the hello and you change `ALPN`.**
- **Presence rides the sync connection** (`Peer::set_focus`, `watched`,
  `present`). A connection becomes a presence link once the peer's hello has
  arrived, and is withdrawn when its task ends, however it ends (`Link`
  drops, so a cancelled task counts). Two devices that dial each other at
  once have two connections, so links are counted, and the survivor starts
  afresh when one goes. A device that vanishes is noticed only when iroh
  gives up on its connection (keepalive 5 s, then the idle timeout): there
  are no timers of ours, so don't lengthen those without thinking about it.
  Epochs come from the clock, so a clock set back can leave a restarted
  device's records ignored.
- **Typing is not streamed to devices that can't see it.** Three signals
  with three jobs: `changes` (`subscribe`) fires on every graph change and
  drives saving, so saving is never held back; `publish` makes connections
  flush; `gate_changes` only makes them re-read the gate's deadline. A local
  edit reported with `edited(block)` publishes at once if another device has
  that block open, and otherwise waits, with whatever follows, until the
  edit ends: `edit_idle` (2 s) after the latest, this device leaving the
  block (`set_focus`), `end_edit`, someone opening the block, or any
  `changed()`. Loro keeps every op however often we publish, so time travel
  is unaffected: only when peers hear differs. The op log is linear, so a
  publish sends everything up to that point, not just the watched block. A
  connection's hello still sends everything, held edits included. The
  timers live in the connection tasks, so there is nothing to spawn.
- **`Blobs` is the bytes of the graph's binaries**, kept in the graph's file by
  `blob_store`. Hashes are BLAKE3. `get` checks `has` first so that an absent
  blob is `Missing`, not a store error. `Blobs::open` makes the file if there is
  none (so it may be opened before the graph is), and moves in an old `blobs/`
  directory. It opens the file through a `Store` whose
  keyring is never asked: the blobs don't need the device's key, so it takes none. Anything `put` is tagged in name only: no tags are kept.
- **Export and import of files are `Blobs::export_assets` and `import_assets`**,
  called by the CLI after the Markdown's. Core does the Markdown and knows
  nothing of the store, and names the file (`Asset::file_name`). Import keeps a
  file only if its name's stem is the BLAKE3 hash *of its contents*, so a
  Logseq `assets/` isn't swept in as bytes nothing refers to, and a corrupt file
  isn't kept under a hash it doesn't have. The CLI opens the store only if
  there is something to do, so an export of a graph with no files does nothing
  to the file.
- **`gc` deletes only what the rule in `gc.rs` allows, and deleting is
  `Blobs::delete`**, one transaction that removes exactly the hashes named. It
  was a collector run on a stopped store while iroh-blobs kept its own database;
  now there is nothing to collect. **The node must still not be serving**, for
  another reason: `gc` saves the graph it loaded, which would be written over
  what a serving node saved since. The old store's lock stopped that by
  accident, and nothing does now. The empty file is built in: `has` is always
  true for it and `stored` never lists it. **The ledger is the `unreferenced`
  table**, saved whole in one transaction, and a text `unreferenced-blobs.txt`
  from before is taken in on the first load (existing rows win, being the later
  sighting) and set aside; a row or a line that can't be read only starts that
  file's clock again. A file goes
  only when unreferenced now, unreferenced for the grace, *and* first seen
  unreferenced at a version `compaction_point` includes; no compaction point,
  no deletion. Versions are per-peer counts, so only versions of the same graph
  compare: a test that builds each in a fresh graph tests nothing.
- **Blobs travel on iroh-blobs' own ALPN, gated by `Peer::permits`**, the
  rule `accept` uses for sync (`Blobs::provider` asks it on every
  connection, in intercept mode, before anything is read; pushes stay
  disabled). **Each device must list the other**: the one that has the
  bytes checks its own list, so `admit` both ways in a test or the fetch
  is refused with `permission`. `Peer::set_blobs` is what turns it on in
  `serve`; without a store nothing is offered or fetched. `fetch_blobs`
  looks at what `missing_blobs` says on a graph change, a connection
  change and every 30 s while anything is missing; it connects by ID, which
  works on localhost because iroh keeps the address of a device that
  connected to it. Don't conclude "addressing" from a failed fetch in a
  test: `connection lost` was the gate refusing.
- **`--local ADDR`** binds to that address only, with no relays or address
  lookup. The tests use it on `127.0.0.1:0`, and iroh tests use
  `presets::Minimal` with relays disabled, so nothing leaves the machine.
