type:: tool

- A shared build cache. CI's Rust builds go through it, [[The shell]]'s included.
- It runs build scripts through a `#!/bin/sh` wrapper. On Ubuntu that is dash, which drops environment variables whose names aren't valid shell identifiers, and that broke Tauri's build. So the shell's CI job runs build scripts directly, with `MBX_BUILD_SCRIPT_EXECUTION=0`, until mbx stops using `sh`.
- It can't cache [[cargo-fuzz]]'s builds: under its sanitizer flags mbx bypasses nearly every compile. The fuzz jobs skip it, through the setup action's `mbx` input, and use [[rust-cache]] instead.
- Each CI job keeps its own cache, named in the key through the setup action's `cache` input, which defaults to the job's name. A job that builds what another does can share its cache: mutation testing uses the rust job's. With one cache for all, every job restored whichever tree a job had saved last, `wasm32` builds and all, and at 3.5 GB that no longer fitted on a runner.
	- The name goes in mbx's cache generation, not its key suffix, because a miss restores from a prefix that the generation is part of and the suffix isn't.
