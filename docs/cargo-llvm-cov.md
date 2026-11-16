type:: tool

- Source-based code coverage for Rust, as a Cargo subcommand that wraps `cargo nextest`. https://github.com/taiki-e/cargo-llvm-cov
- CI runs `cargo llvm-cov nextest` in the `sonarcloud` job to write `lcov.info`, which [[SonarCloud]] imports. It needs the `llvm-tools` component, which the job adds.
- Coverage is reported, not required: [[cargo-mutants]] is what checks that tests notice changes. [[Testing]]
