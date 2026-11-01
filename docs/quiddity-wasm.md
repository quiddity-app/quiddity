type:: crate

- [[quiddity-view]], compiled to WASM: the UI's own replica of the graph. [[The UI]]
- `crates/wasm`. It is a thin layer: the work and the tests are in the view.
- The `wasm-bindgen` crate and its command line tool must be exactly the same version. `mise install` gets the right one. [[mise]]
- A missing value is written as `null`, not `undefined`, which is what JavaScript expects of it.
