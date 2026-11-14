type:: library

- Content-addressed storage and transfer of files by BLAKE3 hash, from the same project as [[iroh]]. https://github.com/n0-computer/iroh-blobs
- [[quiddity-node]] keeps a graph's [[Binaries]] in the graph's [[SQLite]] file, through a store of its own that serves iroh-blobs' storage protocol (`blob_store`, tested against iroh-blobs' in-memory store). Before that they were in iroh-blobs' filesystem store, `blobs/` beside the file, and an old one is moved in. It also moves them between devices, on its own protocol, gated by the device list. [[Blob transfer]]
