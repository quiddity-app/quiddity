type:: tool

- Coverage-guided fuzzing for Rust, with libFuzzer. https://github.com/rust-fuzz/cargo-fuzz
- The targets are `block`, `outline`, `peer` and `query`, in `fuzz/`. A crash becomes a seed and a regression test in the crate it is about. It has found bugs that 20,000 property test cases didn't. [[Testing]]
- CI fuzzes each target for a minute on pull requests and on `main`, with the dependencies' build cached by [[rust-cache]], since [[mbx]] can't cache it.
