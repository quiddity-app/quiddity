# Fuzz targets

`block` (the syntax parser), `outline` (the outline format, to a fixed
point), `peer` (a session fed arbitrary messages) and `query` (the query
parser, to a fixed point, and agreeing with meta-property lines on where a
query ends). The crate is outside
the workspace and needs nightly.

```sh
cargo +nightly fuzz run --target x86_64-unknown-linux-gnu outline \
    fuzz/corpus/outline fuzz/seeds/outline
```

- Pass `--target`: a prebuilt `cargo fuzz` targets musl otherwise.
- `seeds/` is committed and holds every crash found so far, plus examples
  of real input. `corpus/` and `artifacts/` are not committed.
- A crash also gets a regression test in the crate it's about, which
  fails without the fix.
