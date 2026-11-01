type:: tool

- Property testing for Rust: tests that hold for generated inputs, shrunk to a small failing case when they don't. https://github.com/proptest-rs/proptest
- Quiddity tests properties first, with generators weighted towards the inputs that matter. Past failures are kept in `*.proptest-regressions` files and replayed. [[Testing]]
