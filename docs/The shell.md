alias:: [shell, apps/app]

- The [[Tauri]] 2 app that hosts [[The UI]]: `apps/app`. It holds the graph on disk and hands it to the UI, which keeps its own replica, so there is little here by design. [[Architecture]]
- Two commands carry the graph, both raw bytes: `load_graph` returns a snapshot, and `push_updates` imports [[Loro]] updates from the UI, saves and sends them on.
- `subscribe_updates` takes a channel and sends the UI, as raw bytes, whatever other devices have changed since it was last told. The UI imports it. [[App sync]]
- It holds a `Peer` from [[quiddity-node]] and puts it on the network when the app starts. `device_id` and `allow_device` let devices meet; nothing in the UI calls them yet. [[App sync]]
- It opens the one graph in the app's data directory, or the directory `QUIDDITY_GRAPH` names. [[Storage]]
- It opens `http(s)` and `mailto` links in the system's browser, so following one doesn't replace the UI. [[Rendering]]
- It sits outside the Cargo workspace, because it needs Tauri's system libraries, and has its own CI job, whose builds go through [[mbx]].
