# quiddity
Local first, multiplayer, second brain

See [docs/Design.md](docs/Design.md) for the design, and `docs/` for the rest of
the documentation, published as a site (below).
Before changing anything, read [AGENTS.md](AGENTS.md): how we work, and
where to find the notes on each crate.

## Layout

- `crates/syntax`: the block Markdown syntax (properties, links, block
  references). The reference parser.
- `crates/core`: the graph model on Loro, the index (names, link resolution,
  backlinks), property schemas, journals, duplicate merging, and text
  export/import.
- `crates/sync`: the sync protocol, independent of transport.
- `crates/node`: sync over iroh, and `quiddity-node`, a bare-bones command
  line and headless sync node.
- `crates/view`: what a UI shows of a graph (entities, references, favourites,
  journals) and the outliner's edits, tested natively.
- `crates/wasm`: `quiddity-view` compiled to WASM: the UI's own replica of the
  graph.
- `apps/ui`: the Next.js UI (static export): three panes, with entrypoints,
  favourites and recents on the left, the current entity in the middle and
  open secondaries on the right, all editable.
- `apps/app`: the Tauri 2 shell that hosts the UI.
- `fuzz`: fuzz targets for the parsers and the sync protocol.

## Trying it

```sh
cargo run -p quiddity-node -- -g ~/graph init
cargo run -p quiddity-node -- -g ~/graph import ~/logseq-graph
cargo run -p quiddity-node -- -g ~/graph serve --allow <device id>

# on another device
quiddity-node -g ~/graph init --blank    # prints this device's id
quiddity-node -g ~/graph sync <server id>   # join before editing
quiddity-node -g ~/graph export ~/export
quiddity-node -g ~/graph devices           # who holds the graph, and when each last saved
```

Each device keeps only the history some device might still need: what
every device has stored, as far as it knows. A device silent for 30 days
stops holding that back.

## The documentation site

`docs/` is itself a graph. CI imports it with `quiddity-node` and builds the UI
around the result, then publishes that to GitHub Pages. `mise run docs` does
the same locally, into `apps/ui/out`.

## The app

With [mise](https://mise.jdx.dev) the whole thing is one command in a fresh
checkout:

```sh
mise trust           # once, so that mise reads mise.toml
mise run app         # installs what's needed, builds and opens the app
```

The first run installs the toolchain (Rust with the WASM target, Node, pnpm
and a `wasm-bindgen` CLI) and compiles a lot, so it takes a while. On Linux
Tauri also needs system libraries that mise can't install; `mise run doctor`
says what is missing and how to get it (it runs first, so `mise run app` stops
with the same message rather than a compiler error). `mise tasks` lists the
rest: `ui` (the UI alone in a browser), `check` (what CI checks), `test`,
`e2e`, `fmt`, `lint`, `build`, `wasm` and `node`.

Without mise, `pnpm install` and then:

```sh
pnpm ui dev          # the UI alone, in a browser, at http://localhost:3000
pnpm app dev         # the Tauri shell around it (needs Tauri's system packages)
```

The UI embeds the graph replica, which is built from `crates/wasm`, so it needs
the `wasm32-unknown-unknown` Rust target and a `wasm-bindgen` CLI matching the
version in `crates/wasm/Cargo.toml` (`mise install` gets both, the CLI by
compiling it). `pnpm ui wasm`
rebuilds it; `dev`, `build` and `typecheck` do so first.

The shell opens the one graph in the app's data directory, creating it if
need be. Set `QUIDDITY_GRAPH` to a directory made with `quiddity-node` to work
on that graph instead. Favourites are the entities with `favourite:: true`.
Run in a plain browser, the UI works on a made-up graph in memory, which is
reset on reload.

## Testing

Each layer is property-tested on its own, with system tests on top:

| Layer | Tests |
|---|---|
| syntax | generated values round-trip, alone and in prose (`crates/syntax/tests`); arbitrary text is fuzzed |
| outline format | generated outlines round-trip; arbitrary files are fuzzed to a fixed point |
| graph | random edits against an in-memory model |
| index | link resolution and backlinks against a brute-force oracle |
| sync | replicas converge however edits and syncs interleave; additions are never lost; journals merge |
| protocol | sessions over a simulated network with drops and relays; arbitrary peer input is fuzzed |
| system | graph → export → import round trip; real iroh connections; the CLI end to end |

The UI has browser tests (Playwright, against the static export, on the
in-memory graph), which cover editing, undo, journals and references:

```sh
pnpm ui build
pnpm --filter @quiddity/ui exec playwright install chromium   # once
pnpm ui test:e2e
```

```sh
cargo fmt --all --check
cargo clippy --all-targets
cargo nextest run                          # or cargo test
PROPTEST_CASES=5000 cargo nextest run --release   # deeper
cargo +nightly fuzz run block              # or outline, peer
PROPTEST_CASES=32 cargo mutants            # are the tests any good?
```

`cargo mutants` changes the code one small way at a time and checks that
some test notices. Each mutant is tested against its own crate's tests
only, so every layer has to stand up on its own. CI runs it over the lines
each pull request changes.
