type:: tool

- A shared build cache. CI's Rust builds go through it, [[The shell]]'s included.
- It runs build scripts through a `#!/bin/sh` wrapper. On Ubuntu that is dash, which drops environment variables whose names aren't valid shell identifiers, and that broke Tauri's build. So the shell's CI job runs build scripts directly, with `MBX_BUILD_SCRIPT_EXECUTION=0`, until mbx stops using `sh`.
- It can't cache [[cargo-fuzz]]'s builds: under its sanitizer flags mbx bypasses nearly every compile. The fuzz jobs skip it, through the setup action's `mbx` input, and use [[rust-cache]] instead.
- Back to [[Tool]].
