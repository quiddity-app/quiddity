type:: tool

- Installs a project's tools and runs its tasks. https://mise.jdx.dev
- `mise run app` builds and launches the app from a fresh checkout, and `mise tasks` lists the rest. The tasks are in `mise.toml`, and kept in step with CI. [[Running the app]]
- `mise install` gets the toolchain, including the WASM target and the exact `wasm-bindgen` that [[quiddity-wasm]] pins.
