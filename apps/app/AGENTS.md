# apps/app

The Tauri 2 shell. It holds the graph on disk and hands it to the UI; the UI
holds its own replica and answers its own queries (see `apps/ui` and
`crates/wasm`), so there is little here by design.

- **It is outside the Cargo workspace** (`exclude` in the root `Cargo.toml`),
  because it needs Tauri's system packages (webkit) that the core's CI
  shouldn't. It has its own `Cargo.lock`, its own CI job, and its own
  `cargo fmt`, `clippy` and `test` runs (from `src-tauri`).
- **Six commands.** `load_graph` returns a snapshot and `push_updates`
  takes Loro updates as the request body, imports them, saves and sends them
  to other devices, both as raw bytes rather than JSON arrays. Updates that
  arrive out of order are held by Loro until their causes do.
  `push_updates` takes an `x-typed` header naming the blocks typed in, when
  that is all the updates are: the shell calls `Peer::edited` for them and
  `changed` otherwise. `set_focus` takes the block being edited. The UI sends
  the focus after flushing what it typed, so moving on ends the typing.
  `subscribe_updates` is below. `device_id` and `allow_device` are for letting
  another device in (see `docs/App sync.md`); the Devices page calls them.
- **The shell's graph is a `quiddity_node::Peer`, on the network from
  `setup`.** `start_network` binds an endpoint, serves the peer, dials the
  devices the graph lists and saves a moment after any change. It never
  blocks startup: a failure is printed and the app runs without sync.
  `QUIDDITY_NO_SYNC` turns it off and `QUIDDITY_LOCAL` keeps it to one
  address.
- **Changes from other devices reach the UI on a channel.**
  `subscribe_updates` takes a `Channel` and sends raw bytes: the graph's
  updates since what the UI is known to have (`Session::ui_updates`). That
  version is the snapshot's, then everything pushed, then everything sent. A
  UI that asks for the snapshot again has started over, so it replaces the
  version. Saving records the device in the graph, so each push is followed by
  a small update of just that record.
- **Each push saves a whole snapshot**, atomically. That is fine at this size;
  an update log is the fix when it isn't. The UI pushes a few hundred
  milliseconds after typing stops, and on the window being hidden.
- `session.rs` knows nothing about Tauri, so it is tested on its own.
- **`QUIDDITY_GRAPH`** names a directory made by `quiddity-node` to work on
  instead of the one graph in the app's data directory.
- **Permissions are in `capabilities/default.json`.** The opener plugin lets the
  UI open `http(s)` and `mailto` links in the system's browser; without it
  following one would replace the UI with the page.
- **Don't run `cargo generate-lockfile`** here: it re-resolves everything.
  Let cargo add only what a new dependency needs.

## Trying it for real

A debug build loads the dev server unless the frontend is embedded:

```sh
pnpm ui build
cargo build --features tauri/custom-protocol      # in src-tauri
```

There is no display in a sandbox. `xvfb-run` with
`WEBKIT_DISABLE_COMPOSITING_MODE=1` runs the window, `xdotool` clicks and
types in it, and `import -window root` takes a screenshot. Check a saved edit
with `quiddity-node -g <graph> export <dir>` once the app has exited. The
`target` directory is large; `cargo mutants` copies the tree, so delete it
first.
