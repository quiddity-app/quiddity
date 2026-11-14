# quiddity-sync

The sync protocol, independent of transport: a `Session` per connection,
`Message`s, and length-prefixed frames.

- **A session tracks what the peer has** (`theirs`). It starts as the
  version in the peer's hello, then grows with everything sent and received.
  `flush` sends only what the peer lacks. A peer whose version is wrong gets
  too little or too much, never an error, so versions are checked on every
  exchange in core's sync proptest.
- **Schemas must match exactly** for now. A mismatch puts the session in
  `Incompatible` and nothing is exchanged; updates before a hello are an
  error.
- **The hello carries the graph's ID, and two different ones are refused**
  (`State::OtherGraph`, before the history checks, with no vouching). A side
  with none is not refused: a blank replica is joining, and a graph from
  before graphs had IDs may be the same one that has since been given one.
  That leaves a gap: an unidentified graph will still merge with an unrelated
  one, until it is given an ID. Changing the hello's encoding means a new
  ALPN in `quiddity-node`, so old peers are refused at connecting.
- **Compacted peers.** The hello says where the sender's history starts.
  A side behind the other's start (other than a blank one) is `Behind`, and
  the other `PeerBehind`: updates either way could never apply, so nothing
  is exchanged. Following the device list's rule, only an evicted device
  gets there. A blank peer is sent a `Snapshot` instead of updates, as is a
  peer this replica has compacted past since the hello; a snapshot is only
  taken by a replica that has reached where it starts (`Graph::start_of`),
  since anything else would import nothing, silently.
- **The session vouches** for a peer it hasn't heard of (see core's
  `vouch_for`), if it has anything to give. That's why `receive` takes the
  time. A blank replica doesn't.
- **Presence is not in the graph.** `Presence` (`presence.rs`) is ephemeral
  and kept apart so it leaves no history. Each device owns a `Record` (blocks
  open, devices connected to), ordered by (epoch, seq) so a restart outranks
  its past. Gossip works like `Session::flush`: per connected peer, `outgoing`
  sends the live records it hasn't been told. Forwarding "whatever is news"
  does not work: a neighbour that already holds a record never passes it on,
  so a device that forgot it, or never had it, never hears it again. Records
  are therefore never forgotten, and liveness is computed: reachable over
  links that **both** ends list, so a crashed device's stale claim can't keep
  it alive. No timeouts: the transport's own connection loss is the signal.
- **`Message::Presence` is not a session's business**: `Session::receive`
  ignores it, and the node routes it between connections, because presence
  belongs to the device and not to one connection. `Session::peer` gives the
  device ID from the hello that presence is keyed by.
- **Unknown message types are ignored** by the caller (`quiddity-node`), so
  newer peers can add messages. `decode` reports them as `UnknownType`.
- **Frames are capped at `MAX_FRAME`** (256 MiB), checked before buffering.
  A first sync sends a whole graph's history in one frame.
- The simulation test (`tests/simulation.rs`) runs sessions over a network
  that connects and disconnects, delivers messages one at a time, and
  flushes in batches. Settling checks that each connected group agrees,
  which is what catches a session trusting its peer too much.
