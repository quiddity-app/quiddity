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
- `Store::save` is atomic and durable: a uniquely named temporary is flushed, renamed over the snapshot, and the directory flushed; saves are serialised by a mutex shared by clones, and the previous snapshot is kept as `graph.loro.bak`, which `load` falls back to if the snapshot can't be decoded. The device key is written whole then hard-linked into place, so a crash can't leave a partial one. It is created
  once with mode 0600; an unreadable key is reported, never replaced.
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
  the dialler stops them. `address` is called for every attempt: a device
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
  a graph yet.
- **`Peer::sync_state` is what the app's cloud shows** (`docs/Sync status.md`).
  Each connection records what its peer is known to have (`Session::theirs`)
  in `Peer::syncs`, and the state is worked out when asked (`classify`, pure
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
  why even if the close arrives before the peer's hello is read.
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
- **`Blobs` is the bytes of the graph's binaries**, an iroh-blobs `FsStore`
  in `blobs/` beside the snapshot. Hashes are BLAKE3. `get` checks `has`
  first so that an absent blob is `Missing`, not a store error. Call
  `shutdown` before dropping the last clone if something will reopen the
  directory at once: the store's database is locked until its tasks stop.
  Anything `put` is tagged, but tags are not how anything is kept: `sweep`
  drops them all.
- **Export and import of files are `Blobs::export_assets` and `import_assets`**,
  called by the CLI after the Markdown's. Core does the Markdown and knows
  nothing of the store, and names the file (`Asset::file_name`). Import keeps a
  file only if its name's stem is the BLAKE3 hash *of its contents*, so a
  Logseq `assets/` isn't swept in as bytes nothing refers to, and a corrupt file
  isn't kept under a hash it doesn't have. The CLI opens the store only if
  there is something to do, so an export of a graph with no files doesn't leave
  an empty `blobs/` behind, and shuts it down so the next run can open it.
- **`gc` deletes only what the rule in `gc.rs` allows, and deleting is
  `blobs::sweep`.** iroh-blobs deletes by its own collector, which removes
  *everything not marked live*, so `sweep` opens the store with a collector
  whose callback marks every stored blob except the doomed ones, drops the
  tags first (they would keep a doomed blob alive), waits, and checks that what
  it meant to keep is still there. It runs only on a store nothing else has
  open. The empty file is built in: `has` is always true for it and `stored`
  never lists it, so don't read "still there" as "not collected". A file goes
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
- The node crate needs a newer Rust than the rest (see `rust-version`).
