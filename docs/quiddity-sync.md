type:: crate

- The sync protocol, independent of transport: messages, framing, and a session per connection that knows what its peer has. [[Sync]]
- `crates/sync`. [[Presence]] lives here too.
- Tested over a simulated network that drops, scrambles and relays, and fuzzed with arbitrary peer input (the `peer` target). [[Testing]]
