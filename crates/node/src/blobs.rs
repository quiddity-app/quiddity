//! The bytes of a graph's binaries, kept by their BLAKE3 hash.
//!
//! The graph names a binary by hash and describes it in text; the bytes live
//! here, beside the snapshot, in an [`iroh_blobs`] store, so that devices can
//! later exchange them on iroh-blobs' own protocol. Every device keeps every
//! blob for now.

use std::path::Path;

use iroh::EndpointId;
use iroh::endpoint::Connection;
use iroh_blobs::BlobsProtocol;
use iroh_blobs::HashAndFormat;
use iroh_blobs::provider::events::{
    AbortReason, ConnectMode, EventMask, EventSender, ProviderMessage,
};
use iroh_blobs::store::fs::FsStore;

pub use iroh_blobs::Hash;

/// The name of the protocol blobs travel on, negotiated when connecting.
pub use iroh_blobs::ALPN;

const DIR: &str = "blobs";

#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    #[error("blob store: {0}")]
    Store(String),
    #[error("no blob with hash {0}")]
    Missing(Hash),
}

impl BlobError {
    fn store(e: impl std::fmt::Display) -> Self {
        Self::Store(e.to_string())
    }
}

/// The blobs of one graph. Cheap to clone: clones share the store.
#[derive(Debug, Clone)]
pub struct Blobs {
    store: FsStore,
}

impl Blobs {
    /// Opens the blob store in a graph's directory, creating it if need be.
    pub async fn open(graph_dir: &Path) -> Result<Self, BlobError> {
        let store = FsStore::load(graph_dir.join(DIR))
            .await
            .map_err(BlobError::store)?;
        Ok(Self { store })
    }

    /// Keeps `bytes` and returns their hash. Putting the same bytes again
    /// changes nothing.
    pub async fn put(&self, bytes: impl Into<bytes::Bytes>) -> Result<Hash, BlobError> {
        let tag = self
            .store
            .blobs()
            .add_bytes(bytes)
            .await
            .map_err(BlobError::store)?;
        Ok(tag.hash)
    }

    /// The bytes with this hash. They are checked against it as they are read.
    pub async fn get(&self, hash: Hash) -> Result<bytes::Bytes, BlobError> {
        if !self.has(hash).await? {
            return Err(BlobError::Missing(hash));
        }
        self.store
            .blobs()
            .get_bytes(hash)
            .await
            .map_err(BlobError::store)
    }

    /// Whether the whole blob is here, not just part of it.
    pub async fn has(&self, hash: Hash) -> Result<bool, BlobError> {
        self.store.blobs().has(hash).await.map_err(BlobError::store)
    }

    /// The protocol handler that gives blobs to devices that ask, if
    /// `permit` lets them: it is asked for each connection's device before
    /// anything is read. Nothing can be written through it.
    pub fn provider(
        &self,
        permit: impl Fn(&EndpointId) -> bool + Send + Sync + 'static,
    ) -> BlobsProtocol {
        let mask = EventMask {
            connected: ConnectMode::Intercept,
            ..EventMask::DEFAULT
        };
        let (events, mut requests) = EventSender::channel(32, mask);
        tokio::spawn(async move {
            while let Some(message) = requests.recv().await {
                if let ProviderMessage::ClientConnected(connected) = message {
                    let allowed = connected.inner.endpoint_id.as_ref().is_some_and(&permit);
                    let answer = if allowed {
                        Ok(())
                    } else {
                        Err(AbortReason::Permission)
                    };
                    connected.tx.send(answer).await.ok();
                }
            }
        });
        BlobsProtocol::new(&self.store, Some(events))
    }

    /// Fetches a blob from the device at the other end of `connection`,
    /// keeping only what matches `hash`: the bytes are verified as they
    /// arrive, so a device can't give another's blob in its place.
    pub async fn fetch(&self, connection: Connection, hash: Hash) -> Result<(), BlobError> {
        self.store
            .remote()
            .fetch(connection, HashAndFormat::raw(hash))
            .await
            .map_err(BlobError::store)?;
        Ok(())
    }

    /// Stops the store's background tasks, once everything is written.
    pub async fn shutdown(&self) -> Result<(), BlobError> {
        self.store.shutdown().await.map_err(BlobError::store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What goes in comes out, under the BLAKE3 hash of its bytes, and a
    /// store opened again later still has it.
    #[tokio::test]
    async fn bytes_are_kept_by_their_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let blobs = Blobs::open(tmp.path()).await.unwrap();
        let hash = blobs.put(&b"hello"[..]).await.unwrap();
        assert_eq!(hash, Hash::new(b"hello"));
        assert_eq!(blobs.put(&b"hello"[..]).await.unwrap(), hash);
        assert_eq!(&blobs.get(hash).await.unwrap()[..], b"hello");
        blobs.shutdown().await.unwrap();

        let again = Blobs::open(tmp.path()).await.unwrap();
        assert!(again.has(hash).await.unwrap());
        assert_eq!(&again.get(hash).await.unwrap()[..], b"hello");
    }

    /// Empty files and large ones, which iroh-blobs keeps differently
    /// (inline in its database, or in a file of their own).
    #[tokio::test]
    async fn empty_and_large_blobs_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let blobs = Blobs::open(tmp.path()).await.unwrap();
        for len in [0usize, 1, 16 * 1024, 5 * 1024 * 1024 + 3] {
            let data: Vec<u8> = (0..len).map(|i| (i * 31 % 251) as u8).collect();
            let hash = blobs.put(data.clone()).await.unwrap();
            assert_eq!(blobs.get(hash).await.unwrap().to_vec(), data, "{len} bytes");
        }
    }

    /// Asking for what isn't there says so, rather than failing in the store.
    #[tokio::test]
    async fn a_missing_blob_is_reported_as_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let blobs = Blobs::open(tmp.path()).await.unwrap();
        let hash = Hash::new(b"absent");
        assert!(!blobs.has(hash).await.unwrap());
        assert!(matches!(blobs.get(hash).await, Err(BlobError::Missing(h)) if h == hash));
    }
}
