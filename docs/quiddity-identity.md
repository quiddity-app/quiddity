type:: crate

- The identifiers, each its own type, and how each is written down: the device key, device ID, graph ID and node ID. [[Identity]]
- `crates/identity`. It has no randomness and no I/O, so it builds for the browser as well: the caller gives it random bytes.
- The device key is the one secret, and the crate is shaped to make it hard to misuse: no `Display`, `Serialize` or `Clone`, a `Debug` that says nothing, wiped when dropped, and errors that never repeat what they were given. What is used is derived from it with [[BLAKE3]], each use under its own context.
- Text forms are a prefix, base 62 and a CRC-32, in the shape of GitHub's tokens, so that a leaked key can be found. [[Secret scanning]]
- Property-tested: whatever is written reads back, and what reads back writes the same text. Every single-character change and every swap of neighbours to a token is refused. The text form of a value is pinned, so changing it fails a test. Fuzzed with arbitrary text (the `identity` target). [[Testing]]
