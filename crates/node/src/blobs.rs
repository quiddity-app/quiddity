//! The bytes of a graph's binaries, kept by their BLAKE3 hash.
//!
//! The graph names a binary by hash and describes it in text; the bytes live
//! here, beside the snapshot, in an [`iroh_blobs`] store, so that devices can
//! later exchange them on iroh-blobs' own protocol. Every device keeps every
//! blob for now.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use iroh::EndpointId;
use iroh::endpoint::Connection;
use iroh_blobs::BlobsProtocol;
use iroh_blobs::HashAndFormat;
use iroh_blobs::provider::events::{
    AbortReason, ConnectMode, EventMask, EventSender, ProviderMessage,
};
use iroh_blobs::store::fs::FsStore;
use iroh_blobs::store::fs::options::Options;
use iroh_blobs::store::{GcConfig, ProtectOutcome};
use quiddity_core::{Graph, Index};

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
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Sweep(String),
    #[error(transparent)]
    Graph(#[from] quiddity_core::Error),
}

/// What [`Blobs::export_assets`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Exported {
    /// Files written to `assets/`.
    pub written: usize,
    /// Assets the graph names whose bytes are not on this device yet, so
    /// they are not in the export.
    pub missing: usize,
}

/// What [`Blobs::import_assets`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Imported {
    /// Files kept.
    pub stored: usize,
    /// Files in `assets/` that are not named for the hash of what is in
    /// them, and so are not Quiddity's: left alone.
    pub skipped: usize,
}

impl BlobError {
    pub(crate) fn store(e: impl std::fmt::Display) -> Self {
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

    /// Writes the bytes of the graph's assets to `<dir>/assets/<hash>.<ext>`
    /// (see `Asset::file_name`), once for each hash, so a text export can
    /// refer to them by path and be read by anything. An asset whose bytes
    /// are not here yet is counted, not written. Whole files are read into
    /// memory, one at a time.
    pub async fn export_assets(&self, graph: &Graph, dir: &Path) -> Result<Exported, BlobError> {
        let index = Index::build(graph)?;
        let assets = dir.join("assets");
        let mut seen = HashSet::new();
        let mut done = Exported::default();
        for (_, asset) in index.assets() {
            let Ok(hash) = asset.hash.parse::<Hash>() else {
                continue;
            };
            if !seen.insert(hash) {
                continue;
            }
            if !self.has(hash).await? {
                done.missing += 1;
                continue;
            }
            let bytes = self.get(hash).await?;
            std::fs::create_dir_all(&assets)?;
            std::fs::write(assets.join(asset.file_name()), bytes)?;
            done.written += 1;
        }
        Ok(done)
    }

    /// Keeps the files in `<dir>/assets/` that are named for the BLAKE3 hash
    /// of what is in them, as an export names them. Others are skipped: a
    /// Logseq graph's `assets/` is full of files nothing in Quiddity refers
    /// to by hash, and one whose name doesn't match its contents is damaged or
    /// not ours. There being no `assets/` is not an error.
    pub async fn import_assets(&self, dir: &Path) -> Result<Imported, BlobError> {
        let mut done = Imported::default();
        let entries = match std::fs::read_dir(dir.join("assets")) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(done),
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            let named = path
                .file_stem()
                .and_then(|s| s.to_str())
                // Only a name of 64 characters, which `Hash` reads as lower-case hex
                // and nothing else: any other length it would try as base32.
                .filter(|s| s.len() == 64)
                .and_then(|s| s.parse::<Hash>().ok());
            let Some(named) = named else {
                done.skipped += 1;
                continue;
            };
            let bytes = std::fs::read(&path)?;
            if Hash::new(&bytes) != named {
                done.skipped += 1;
                continue;
            }
            self.put(bytes).await?;
            done.stored += 1;
        }
        Ok(done)
    }

    /// Every blob that is wholly here.
    pub async fn stored(&self) -> Result<BTreeSet<Hash>, BlobError> {
        let hashes = self
            .store
            .blobs()
            .list()
            .hashes()
            .await
            .map_err(BlobError::store)?;
        Ok(hashes.into_iter().collect())
    }

    /// Stops the store's background tasks, once everything is written.
    pub async fn shutdown(&self) -> Result<(), BlobError> {
        self.store.shutdown().await.map_err(BlobError::store)
    }
}

/// Deletes the blobs in `doomed` from the store in `graph_dir`, and nothing
/// else.
///
/// iroh-blobs deletes only by garbage collection, which removes everything
/// that is not marked live, and it takes its marks from tags and from a
/// callback. So this opens the store with a collector whose callback marks
/// every blob that is stored but not doomed, having first removed the tags
/// (which would keep a doomed blob alive, and are not how this store keeps
/// anything), waits for the collector to run, and then checks that everything
/// it meant to keep is still there. Nothing is added while it runs: the store's
/// database allows one process, so a node that is serving has to be stopped.
///
/// A blob that was only partly fetched is not stored, in this sense, and goes
/// too; fetching it begins again.
pub async fn sweep(graph_dir: &Path, doomed: &BTreeSet<Hash>) -> Result<(), BlobError> {
    if doomed.is_empty() {
        return Ok(());
    }
    // First, without a collector, to see what is there and let go of the tags.
    let before = {
        let blobs = Blobs::open(graph_dir).await?;
        let stored = blobs.stored().await?;
        blobs
            .store
            .tags()
            .delete_all()
            .await
            .map_err(BlobError::store)?;
        blobs.shutdown().await?;
        stored
    };
    let keep: HashSet<Hash> = before.difference(doomed).copied().collect();
    let protect = keep.clone();

    let mut options = Options::new(&graph_dir.join(DIR));
    options.gc = Some(GcConfig {
        interval: Duration::from_millis(50),
        add_protected: Some(Arc::new(move |live: &mut HashSet<Hash>| {
            live.extend(protect.iter().copied());
            Box::pin(std::future::ready(ProtectOutcome::Continue))
        })),
    });
    let store = FsStore::load_with_opts(graph_dir.join(DIR).join("blobs.db"), options)
        .await
        .map_err(BlobError::store)?;
    let blobs = Blobs { store };

    let gone = async {
        loop {
            let mut remaining = false;
            for hash in doomed.intersection(&before) {
                remaining |= blobs.has(*hash).await?;
            }
            if !remaining {
                return Ok::<(), BlobError>(());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    };
    let waited = tokio::time::timeout(Duration::from_secs(60), gone).await;
    // Whatever happened, what was to be kept must be there.
    let mut lost = Vec::new();
    for hash in &keep {
        if !blobs.has(*hash).await.unwrap_or(false) {
            lost.push(hash.to_string());
        }
    }
    blobs.shutdown().await?;
    if !lost.is_empty() {
        return Err(BlobError::Sweep(format!(
            "the collector removed blobs it was told to keep: {}",
            lost.join(", ")
        )));
    }
    match waited {
        Ok(result) => result,
        Err(_) => Err(BlobError::Sweep(
            "the collector did not remove the blobs in time".into(),
        )),
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
