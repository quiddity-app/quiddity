type:: crate
alias:: headless node

- Sync over [[iroh]] (`Peer`), a graph's directory on disk (`Store`), and the `quiddity-node` command line. [[Sync]] [[Storage]]
- `crates/node`.
- The headless node is `quiddity-node serve`, left running: a full peer owned by the user, not a relay, and no different from any other device. Being always on, it lets devices that are never online together sync. It will keep every blob, once there are any. [[Binaries]] [[Architecture]]
- Commands
	- `init` makes a graph, or with `--blank` an empty replica to sync an existing one into. `id` prints this device's ID.
	- `import` and `export` read and write Markdown. [[Export and import]]
	- `allow` lists a device in the graph, so it may connect, and `devices` lists the devices holding the graph with when each last saved. [[Device list]]
	- `serve` keeps the graph online until interrupted, and `sync` syncs with one device until neither has anything more to send.
- The library starts a peer on the network in one call, for the command line and [[The shell]] alike: `serve` binds an endpoint, serves the peer and dials the listed devices, and `save_on_change` saves a few hundred milliseconds after changes.
	- `Peer::trust` lets a device in until the peer stops without listing it, which is how a blank replica lets in the device it joins through. [[Device list]]
	- `mise run node -- --help` runs it from a checkout. [[mise]]
- Saving records this device and compacts what it writes. [[Compaction]]
- It also holds the [[Publish gate]], and routes [[Presence]] between connections.
- It says how the device stands with the others (`Peer::sync_state`), for [[The shell]] to tell the UI: alone, offline, behind, synced or present. [[Sync status]]
