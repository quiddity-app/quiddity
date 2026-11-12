type:: library

- Content-addressed storage and transfer of files by BLAKE3 hash, from the same project as [[iroh]]. https://github.com/n0-computer/iroh-blobs
- [[quiddity-node]] keeps a graph's [[Binaries]] in its filesystem store, `blobs/` beside the graph's file. Planned, not built: in the graph's [[SQLite]] file instead, through a store of its own that serves iroh-blobs' storage protocol (`blob_store`, built and tested against iroh-blobs' in-memory store, but not yet used). It keeps and serves whole blobs so far; taking one in from another device is the next piece. It also moves them between devices, on its own protocol, gated by the device list. [[Blob transfer]]
