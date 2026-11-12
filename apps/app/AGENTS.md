# apps/app

The Tauri 2 shell. It holds the graph on disk and hands it to the UI; the UI
holds its own replica and answers its own queries (see `apps/ui` and
`crates/wasm`), so there is little here by design.

- **It is outside the Cargo workspace** (`exclude` in the root `Cargo.toml`),
  because it needs Tauri's system packages (webkit) that the core's CI
  shouldn't. It has its own `Cargo.lock`, its own CI job, and its own
  `cargo fmt`, `clippy` and `test` runs (from `src-tauri`).
- **Eleven commands.** `load_graph` returns a snapshot and `push_updates`
  takes Loro updates as the request body, imports them, saves and sends them
  to other devices, both as raw bytes rather than JSON arrays. Updates that
  arrive out of order are held by Loro until their causes do.
  `push_updates` takes an `x-typed` header naming the blocks typed in, when
  that is all the updates are: the shell calls `Peer::edited` for them and
  `changed` otherwise. `set_focus` takes the block being edited. The UI sends
  the focus after flushing what it typed, so moving on ends the typing.
  `subscribe_status` takes a `Channel` and sends how this device stands with
  the others (`Session::sync_state`) each time it changes.
  `subscribe_updates` is below. `device_id` and `allow_device` are for letting
  another device in (see `docs/App sync.md`); the Devices page calls them.
  `put_blob` takes a file as the request body and returns its hash, and
  `get_blob` returns the bytes for a hash: the shell only keeps bytes, and the
  UI says in the graph what a file is, with an `##asset` line, as an ordinary
  edit pushed with `push_updates`.
- **A graph that won't open doesn't stop the app.** `setup` records the
  problem in `Startup` (always managed) and manages no `Session`; the UI asks
  `startup_problem` first and shows the recovery screen instead of calling
  anything else (`docs/Recovery.md`). `recover` takes `retry`, `blank` or
  `fresh`, refuses unless there is a problem (starting again moves the files
  aside, so a graph that opened must never reach it), and then does what `setup`
  would have (`begin`: binaries, then the network) and manages the session.
  The other commands need a `Session` and aren't called while there is none.
  Files are set aside, never deleted.
- **The shell's graph is a `quiddity_node::Peer`, on the network from
  `setup`.** `start_network` binds an endpoint, serves the peer, dials the
  devices the graph lists and saves a moment after any change. It never
  blocks startup: a failure is logged and the app runs without sync.
  `QUIDDITY_NO_SYNC` turns it off and `QUIDDITY_LOCAL` keeps it to one
  address. `QUIDDITY_LOG` sets the log level (default warnings).
  **`Session::keep_blobs` must come before `serve`**: `serve` offers and
  fetches blobs only if the peer already has a store, and nothing says so if
  it doesn't. A store that won't open is logged as a warning and the app
  runs without binaries.
- **Changes from other devices reach the UI on a channel.**
  `subscribe_updates` takes a `Channel` and sends raw bytes: the graph's
  updates since what the UI is known to have (`Session::ui_updates`). That
  version is the snapshot's, then everything pushed, then everything sent. A
  UI that asks for the snapshot again has started over, so it replaces the
  version. Saving records the device in the graph, so each push is followed by
  a small update of just that record.
- **Each push saves a whole snapshot**, in one SQLite transaction. That is fine at this size;
  an update log is the fix when it isn't. The UI pushes a few hundred
  milliseconds after typing stops, and on the window being hidden.
- `session.rs` knows nothing about Tauri, so it is tested on its own.
- **`QUIDDITY_GRAPH`** names a directory made by `quiddity-node` to work on
  instead of the one graph in the app's data directory.
- **The window's background follows the system theme** (`appearance.rs`):
  the page's own ground, so a dark window doesn't open with a flash of white,
  repainted on `ThemeChanged`. The two colours are `--surface` in
  `apps/ui/app/globals.css`, and a test reads the stylesheet and fails if they
  drift. The title bar is the system's and isn't set here.
- **Every icon is made from the SVGs in `src-tauri/icons/source`** by
  `mise run icons` (`scripts/icons.sh`): don't edit the generated PNGs, ICNS
  and ICO by hand. **The Q's ring, not the whole glyph, is centred** in the
  square and in the Android foreground, so a circular mask shows a ring in the
  middle; `apps/ui/e2e/icon.spec.ts` measures the sources and the generated
  launcher icons. Tauri composes the older Android icons from the layers with
  the ring off to one side, so the script takes those from the master alone.
  `apps/ui/app/icon.svg` is a copy of `icon.svg`, for the page's favicon.
- **Permissions are in `capabilities/default.json`.** The opener plugin lets the
  UI open `http(s)` and `mailto` links in the system's browser; without it
  following one would replace the UI with the page.
- **The macOS icon has its shape baked in.** iOS rounds an icon itself, but
  macOS draws an `.icns` as it is, so `icon.icns` comes from
  `source/icon-macos.svg` (Apple's 824px squircle on the 1024px canvas, with a
  shadow) and not from the plain master. `scripts/icons.sh` replaces the one
  Tauri made from the master with it, so don't copy the `.icns` from a bare
  `tauri icon` run.
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
