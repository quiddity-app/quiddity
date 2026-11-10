type:: tool

- Coverage-guided fuzzing for Rust, with libFuzzer. https://github.com/rust-fuzz/cargo-fuzz
- The targets are `block`, `outline`, `peer` and `query`, in `fuzz/`. A crash becomes a seed and a regression test in the crate it is about. It has found bugs that 20,000 property test cases didn't. [[Testing]]
- CI fuzzes each target for a minute on pull requests and on `main`, with the dependencies' build cached by [[rust-cache]], since [[mbx]] can't cache it. It builds with a nightly pinned to a day (`FUZZ_TOOLCHAIN` in `ci.yml`), which Renovate bumps weekly from the dates in Rust's published manifest list (`renovate.json`). A bump changes the CI configuration, so every target fuzzes on the pull request and again on `main`, which saves the cache for the next pull requests. A target is skipped when nothing it depends on has changed: `scripts/fuzz-targets.mjs` works that out, and [[Testing]] says what counts.
