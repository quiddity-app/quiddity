# apps/app

The Tauri 2 shell. It holds the graph on disk and hands it to the UI; the UI
holds its own replica and answers its own queries (see `apps/ui` and
`crates/wasm`), so there is little here by design.

- **It is a workspace member, but not a default one** (`default-members` in
  the root `Cargo.toml`), because it needs Tauri's system packages (webkit)
  that the core's CI shouldn't. Plain `cargo` from the root leaves it out, so
  `--workspace` or `-p quiddity-app` (or running from `src-tauri`) includes it.
  It shares the root `Cargo.lock` and `target`, and has its own CI job. Cargo
  resolves against the lowest `rust-version` in the workspace, which is why
  there is one, in the root `Cargo.toml`, and the app and `quiddity-node`
  inherit it.
- **Nineteen commands.** `load_graph` returns a snapshot and `push_updates`
  takes Loro updates as the request body, imports them, saves and sends them
  to other devices, both as raw bytes rather than JSON arrays. Updates that
  arrive out of order are held by Loro until their causes do.
  `push_updates` takes an `x-typed` header naming the blocks typed in, when
  that is all the updates are: the shell calls `Peer::edited` for them and
  `changed` otherwise. `set_focus` takes the block being edited. The UI sends
  the focus after flushing what it typed, so moving on ends the typing.
  `subscribe_status` takes a `Channel` and sends how this device stands with
  the others (`Session::sync_details`: the state's word and what it rests on, as
  an object, including this device's ID and the network log) each time it changes.
  `subscribe_updates` is below. `network_changed` calls the endpoint's
  `network_change` (`Running` keeps the endpoint, filled and emptied by the
  network task so a graph switched away from leaves none): the UI says it when the page is
  shown again or the browser reports `online`, because a phone's sockets and relay connection don't survive the
  screen being off and iroh can't see that on Android. A woken phone was seen
  dialling for minutes without connecting; this is the remedy iroh documents for
  it, and not yet confirmed on a phone. It then calls `Peer::retry_now`, so the
  dialler tries what it was waiting out at once (there is no session until a
  graph is chosen, so it asks with `try_state`). The endpoint is nothing with the
  network off.
  `device_id` and `allow_device` are for letting
  another device in (see `docs/App sync.md`); the Devices page calls them.
  `list_graphs`, `switch_graph` and `create_graph` are for the UI's graph picker
  (below). `set_entity_open` says whether the UI has an entity open, for the macOS
  menu (below). `put_blob` takes a file as the request body and returns its hash, and
  `get_blob` returns the bytes for a hash: the shell only keeps bytes, and the
  UI says in the graph what a file is, with a block that starts with a `##blob`
  line, as an ordinary edit pushed with `push_updates`.
- **A device with no graph is not given one** (`docs/First run.md`).
  `setup` uses `Session::find`, which says there is none instead of making one,
  and manages `FirstRun` (always managed, like `Startup`) holding where it will
  go. The UI asks `first_run` before anything else and shows the choice;
  `start_graph` takes `new` or `join` (`Start`), refuses unless `FirstRun` is
  set, and then does what `setup` would have. `join` is `Session::create(…, blank = true)`, a
  blank replica, and `is_joining` says whether it still has no history, which is
  what the UI's waiting screen asks. **Starting a graph of its own by default is
  the bug this replaced**: two devices that each did it hold graphs that never
  merge. `Session::open` still makes one if there is none, for `recover` and the
  tests; `setup` must not use it.
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
- **Each push saves the updates since the last**, appended to the file's update
  log in one SQLite transaction; the snapshot is only rewritten when the log
  would pass half its size (see `crates/node/AGENTS.md`). The UI pushes a few hundred
  milliseconds after typing stops, and on the window being hidden.
- `session.rs` knows nothing about Tauri, so it is tested on its own.
- **Graphs are directories under `graphs/` in the app's data directory**
  (`library.rs`, which knows nothing of Tauri), and `current-graph` names the
  one to open at startup. The graph from before there could be several
  (`graph/`) is renamed to `graphs/Quiddity`. **Switching opens the new graph
  first**, so one that won't open changes nothing, then saves the old one and
  stops what ran for it (`Running`: the UI's subscriptions, and the network,
  whose endpoint is shut down and awaited), and puts the new `Session` in the
  same managed state. **Everything per-session must be registered in
  `Running`**, or the old session stays alive holding its graph's binaries and
  switching back fails. The UI reloads afterwards.
- **`QUIDDITY_GRAPH`** names a graph made by `quiddity-node` (a directory, or its
  `.quiddity` file) to work on instead of the graphs in the data directory. It
  is then the only graph: the commands that make or open another refuse.
- **The device's key is in the app's config directory**, not the graph's
  (`QUIDDITY_CONFIG_DIR` overrides it). Two instances on one machine that
  should be two devices need their graphs apart, which gives each its own
  salt and so its own device ID even if the key is shared; they may share the
  key.
- **The window's background follows the system theme** (`appearance.rs`):
  the page's own ground, so a dark window doesn't open with a flash of white,
  repainted on `ThemeChanged`. The two colours are `--surface` in
  `apps/ui/app/globals.css`, and a test reads the stylesheet and fails if they
  drift. The title bar is the system's and isn't set here.
- **The macOS menu bar** (`menu.rs`) is Tauri's default with Delete Entity
  added to File and a Go menu after it. It only emits the item's ID on the
  `menu` event; the UI does the work, since it knows what is open. Delete
  Entity has no shortcut on purpose: Cmd-Backspace is "delete to the start of
  the line" in a block. The File menu is found by its title, as Tauri gives it
  no ID, and made if it isn't there. **The menu code is macOS-only and was
  type-checked on Linux with the cfg lifted, not run**; a test holds the Go
  menu's paths to `entrypoints` in `apps/ui/lib/nav.ts`.
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
