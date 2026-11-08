type:: crate
alias:: headless node

- Sync over [[iroh]] (`Peer`), a graph's directory on disk (`Store`), the bytes of its binaries (`Blobs`), and the `quiddity-node` command line. [[Sync]] [[Storage]]
- `crates/node`.
- The headless node is `quiddity-node serve`, left running: a full peer owned by the user, not a relay, and no different from any other device. Being always on, it lets devices that are never online together sync. It keeps every blob. [[Binaries]] [[Architecture]]
- Commands
	- `init` makes a graph, or with `--blank` an empty replica to sync an existing one into. `id` prints this device's ID.
	- `import` and `export` read and write Markdown. [[Export and import]]
	- `allow` lists a device in the graph, so it may connect, and `devices` lists the devices holding the graph with when each last saved. [[Device list]]
	- `serve` keeps the graph online until interrupted, and `sync` syncs with one device until neither has anything more to send.
- Logging
	- The crates log with `tracing`; only the binaries install a subscriber, which writes to stderr. What a command reports (`imported 2 entities`, an ID, the device list) is its output and goes to stdout whatever the log level, so `-q` never hides it.
	- `quiddity-node` takes `-v` and `-q`: warnings by default, `-v` adds info, `-vv` debug and `-vvv` trace. At `-v` the connections coming and going are logged; at `-vv` the store and import and export, with the directory they work on. [[Loro]] logs at info too, so `-v` is noisy while saving.
	- The app logs warnings and errors by default; `QUIDDITY_LOG=debug` (a level) turns up the rest. [[The shell]]
	- Levels: `error` for what the process can't recover from (saving failed), `warn` for what someone should look at (a refused device, a sync that ended in error, an import that found no files), `info` for lifecycle (connected, a device key created), `debug` for operations and decisions, `trace` for each file or message. A library function returns its error and does not log it; the caller that handles it does. Spans go on operations, never on a function that is called per edit or that takes document text, since a span records its arguments.
	- A missing graph is `StoreError::Missing`, which names the directory, so `import` into a directory that holds no graph says to `init` it.
- The library starts a peer on the network in one call, for the command line and [[The shell]] alike: `serve` binds an endpoint, serves the peer and dials the listed devices, and `save_on_change` saves a few hundred milliseconds after changes.
	- `Peer::trust` lets a device in until the peer stops without listing it, which is how a blank replica lets in the device it joins through. [[Device list]]
	- `mise run node -- --help` runs it from a checkout. [[mise]]
- Saving records this device and compacts what it writes. [[Compaction]]
- It also holds the [[Publish gate]], and routes [[Presence]] between connections.
- It says how the device stands with the others (`Peer::sync_state`), for [[The shell]] to tell the UI: alone, offline, behind, synced or present. [[Sync status]]
- A peer given a blob store offers its blobs to the devices that may connect, and fetches the ones the graph names that it lacks. [[Blob transfer]]
