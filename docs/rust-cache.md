type:: tool

- A GitHub Action that caches a Rust build's dependencies between CI runs, keyed on rustc's version and the lockfile. https://github.com/Swatinem/rust-cache
- The fuzz jobs use it because [[mbx]] can't cache [[cargo-fuzz]]'s builds: `main` saves the dependencies' build and pull requests restore it. `main` only fuzzes, and so only saves, when something a target depends on changed, so the cache can age out between such changes; a pull request that finds no cache builds the dependencies afresh, which takes minutes rather than the minute of fuzzing, so the fuzz job's timeout allows for it. It keys on the nightly toolchain that cargo-fuzz builds with, not the stable one [[mise]] provides. [[Testing]]
- The [[GitHub Pages]] build uses it too. [[Publishing]]
