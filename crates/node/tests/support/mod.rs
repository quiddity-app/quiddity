//! What the blob store's tests share.
#![allow(dead_code)]

use quiddity_node::store::Store;

pub fn graph() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store.init(false).unwrap();
    (dir, store)
}

pub fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Sizes either side of what a chunk (1 KiB) and a chunk group (16 KiB) hold,
/// where the outboard starts and the tree changes shape.
pub const EDGES: [usize; 9] = [
    0,
    1,
    1024,
    1025,
    16 * 1024,
    16 * 1024 + 1,
    40_000,
    65_536,
    200_001,
];

/// Bytes that don't repeat: a pattern with a short period would make a read
/// at the wrong offset return what the right one does.
pub fn bytes(len: usize, seed: u8) -> Vec<u8> {
    let mut state = 0x9E37_79B9_7F4A_7C15u64 ^ u64::from(seed);
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}
