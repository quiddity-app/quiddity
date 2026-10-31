type:: tool

- A shared build cache. CI's Rust builds go through it, [[The shell]]'s included.
- It runs build scripts through a `#!/bin/sh` wrapper. On Ubuntu that is dash, which drops environment variables whose names aren't valid shell identifiers, and that broke Tauri's build. So the shell's CI job runs build scripts directly, with `MBX_BUILD_SCRIPT_EXECUTION=0`, until mbx stops using `sh`.
- Back to [[Tool]].
