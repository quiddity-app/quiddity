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
- `Store::save` is atomic (write then rename). The device key is created
  once with mode 0600; an unreadable key is reported, never replaced.
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
- **`--local ADDR`** binds to that address only, with no relays or address
  lookup. The tests use it on `127.0.0.1:0`, and iroh tests use
  `presets::Minimal` with relays disabled, so nothing leaves the machine.
- The node crate needs a newer Rust than the rest (see `rust-version`).
