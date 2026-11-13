# quiddity-identity

Identifiers, each its own type, and how each is written down. Design:
`docs/Identity.md`.

- **There is one secret, `DeviceKey`, and it can only be misused on purpose.**
  No `Display`, `Serialize` or `Clone`; `Debug` says nothing; wiped on drop;
  errors never repeat the text they were given. Keep it that way: a new
  derive on it, or a method that returns its bytes, defeats the crate. What
  is used is derived (`endpoint_seed`, `node_mask`), each under its own
  BLAKE3 context string. A new use gets a new context.
- **No randomness here.** Constructors take the bytes, so the crate builds
  for WASM and the caller chooses the source. Bytes must be random: a
  `GraphId` or `Salt` that is not unique defeats what it is for.
- **A token has one spelling.** Prefix, payload in base 62 at a fixed width,
  six characters of CRC-32. Decoding re-encodes in a property test, so a
  second spelling of one value is a failure. Changing the alphabet, widths or
  checksum changes stored text: it needs a new prefix, not an edit.
- **The salt is per graph file, not the graph ID**, because a blank replica
  needs a device ID before it knows the graph's.
- **A `SealedNodeId` tells a copy from the original, and nothing else.** The
  mask is derived from a key that sits beside the graph on the same device;
  whoever copies both is the same device.
