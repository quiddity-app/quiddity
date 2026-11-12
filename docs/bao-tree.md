type:: crate

- BLAKE3 verified streaming: a file's hash tree, so that any part of it can be sent with a proof and checked on arrival against the file's hash alone. https://github.com/n0-computer/bao-tree
- [[iroh-blobs]] is built on it, and so is the SQLite blob store in [[quiddity-node]], which keeps a file's hash tree (its outboard) beside its bytes. [[Blob transfer]]
