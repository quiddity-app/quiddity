//! A blob store for iroh-blobs that keeps complete blobs in the graph's
//! SQLite file.
//!
//! iroh-blobs' `Store` is a client of a command channel, and its own stores
//! (`FsStore`, `MemStore`) are actors serving that channel. This is a third,
//! built on the same protocol: iroh keeps doing the transfer and the
//! verification, and what is kept is a row in the `blob` table. It is shaped
//! after iroh-blobs' `MemStore` (MIT OR Apache-2.0), which is the shortest
//! complete implementation, with these differences:
//!
//! - **Only whole blobs are in the file**, and for now only whole blobs are
//!   served: taking a blob in from another device piece by piece is not here
//!   yet.
//! - **Reads are incremental.** The outboard (the hashes that let a part of a
//!   blob be proved) is read whole, being small, and the data is read as far
//!   as is asked for, through SQLite's incremental blob I/O.
//! - **No tags are kept, and temp tags protect nothing.** Nothing here is
//!   deleted because it is untagged: the graph says what is wanted, and what
//!   isn't is deleted by name ([`SqliteBlobs::delete`]). Asking for a tag
//!   works, and the tag is not kept, so listing tags finds none.
//! - **Only bytes are imported.** Importing from a stream or a path, and
//!   exporting to a path, are refused: the node reads and writes files itself.
//! - **A blob over the limit is refused**, [`MAX_BLOB`] unless the store is
//!   opened with another, which is SQLite's limit on one value, so that no
//!   import fails halfway through writing.

use std::io;
use std::sync::{Arc, Mutex, PoisonError};

use bao_tree::io::mixed::{EncodedItem, ReadBytesAt, traverse_ranges_validated};
use bao_tree::io::outboard::PreOrderMemOutboard;
use bao_tree::io::sync::Outboard;
use bao_tree::io::{EncodeError, Leaf};
use bao_tree::{BaoTree, ChunkRanges, TreeNode, blake3};
use bytes::Bytes;
use iroh_blobs::api::blobs::{AddProgressItem, Bitfield, BlobStatus, ExportProgressItem};
use iroh_blobs::api::proto::{
    BatchMsg, BatchResponse, BlobDeleteRequest, BlobStatusMsg, BlobStatusRequest, Command,
    CreateTagMsg, CreateTempTagMsg, DeleteBlobsMsg, DeleteTagsMsg, ExportBaoMsg, ExportBaoRequest,
    ExportPathMsg, ExportRangesItem, ExportRangesMsg, ExportRangesRequest, ImportBaoMsg,
    ImportByteStreamMsg, ImportBytesMsg, ImportBytesRequest, ImportPathMsg, ListBlobsMsg,
    ListTagsMsg, ObserveMsg, ObserveRequest, RenameTagMsg, Scope, SetTagMsg, ShutdownMsg,
    SyncDbMsg, WaitIdleMsg,
};
use iroh_blobs::api::{self, Tag, TempTag};
use iroh_blobs::protocol::ChunkRangesExt;
use iroh_blobs::store::IROH_BLOCK_SIZE;
use iroh_blobs::store::mem::MemStore;
use iroh_blobs::{BlobFormat, Hash, HashAndFormat};
use irpc::channel::{mpsc, oneshot};
use range_collections::range_set::RangeSetRange;
use rusqlite::{Connection, OptionalExtension, params};
use tokio::task::JoinSet;
use tracing::{error, trace};

/// The largest blob kept, unless the store is opened with another limit:
/// SQLite's limit on the length of one value, a billion bytes by default,
/// less 1,024 for the row around it.
pub const MAX_BLOB: u64 = 999_998_976;

/// The blob store, as iroh-blobs' `Store` (which this derefs to) and what
/// iroh keeps to itself: deleting. iroh-blobs deletes only from its own
/// garbage collector, so a store that is never collected can't be told to
/// forget a blob by a client of the protocol.
#[derive(Debug, Clone)]
pub struct SqliteBlobs {
    store: api::Store,
    shared: Arc<Shared>,
}

impl std::ops::Deref for SqliteBlobs {
    type Target = api::Store;

    fn deref(&self) -> &api::Store {
        &self.store
    }
}

impl SqliteBlobs {
    /// Removes these blobs from the file. Naming one that isn't there is not
    /// an error.
    pub async fn delete(&self, hashes: Vec<Hash>) -> io::Result<()> {
        self.shared.delete(hashes).await
    }
}

/// Opens the blob store in a graph's file, checking that it is ours.
pub fn open(store: &crate::store::Store) -> Result<SqliteBlobs, crate::StoreError> {
    open_limited(store, MAX_BLOB)
}

/// Opens the blob store refusing blobs over `max_blob` bytes, which is how
/// the tests reach the limit without a gigabyte of data.
pub fn open_limited(
    store: &crate::store::Store,
    max_blob: u64,
) -> Result<SqliteBlobs, crate::StoreError> {
    Ok(spawn(store.connection()?, max_blob))
}

/// Starts the store on the file's connection. The `Store` inside is a
/// `MemStore` handle, which is only a client of the channel the actor serves:
/// it keeps nothing itself.
pub fn spawn(connection: Connection, max_blob: u64) -> SqliteBlobs {
    let (sender, commands) = tokio::sync::mpsc::channel(32);
    let shared = Arc::new(Shared {
        db: Db(Arc::new(Mutex::new(connection))),
        max_blob,
    });
    tokio::spawn(
        Actor {
            commands,
            shared: shared.clone(),
            tasks: JoinSet::new(),
            idle: Vec::new(),
        }
        .run(),
    );
    SqliteBlobs {
        store: MemStore::from_sender(sender.into()).into(),
        shared,
    }
}

// ---------------------------------------------------------------------
// The file

/// The file's connection, used from blocking tasks one at a time.
#[derive(Debug, Clone)]
struct Db(Arc<Mutex<Connection>>);

impl Db {
    async fn with<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
    ) -> io::Result<T> {
        let db = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let mut connection = db.lock().unwrap_or_else(PoisonError::into_inner);
            f(&mut connection)
        })
        .await
        .map_err(io::Error::other)?
        .map_err(io::Error::other)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// What is said of an import or export this store doesn't do.
fn unsupported(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{what} is not supported by the blob store in the graph's file"),
    )
}

fn store_error(e: io::Error) -> api::Error {
    api::Error::io(e.kind(), e.to_string())
}

#[derive(Debug)]
struct Shared {
    db: Db,
    /// The largest blob kept.
    max_blob: u64,
}

impl Shared {
    async fn delete(&self, hashes: Vec<Hash>) -> io::Result<()> {
        self.db
            .with(move |c| {
                let tx = c.transaction()?;
                for hash in hashes {
                    tx.execute(
                        "DELETE FROM blob WHERE hash = ?1",
                        params![hash.as_bytes().as_slice()],
                    )?;
                }
                tx.commit()
            })
            .await
    }

    /// Where a blob is, if it is anywhere.
    async fn source(&self, hash: Hash) -> io::Result<Option<Source>> {
        if hash == Hash::EMPTY {
            return Ok(Some(Source::Empty));
        }
        let found = self
            .db
            .with(move |c| {
                c.query_row(
                    "SELECT id, size, outboard FROM blob WHERE hash = ?1",
                    params![hash.as_bytes().as_slice()],
                    |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, Vec<u8>>(2)?,
                        ))
                    },
                )
                .optional()
            })
            .await?;
        Ok(found.map(|(id, size, outboard)| Source::Complete {
            db: self.db.clone(),
            id,
            size: size as u64,
            outboard: outboard.into(),
        }))
    }
}

/// Somewhere a blob can be read from.
#[derive(Debug, Clone)]
enum Source {
    /// The empty blob, which is always here and never stored.
    Empty,
    Complete {
        db: Db,
        id: i64,
        size: u64,
        outboard: Bytes,
    },
}

impl Source {
    fn bitfield(&self) -> Bitfield {
        match self {
            Self::Empty => Bitfield::complete(0),
            Self::Complete { size, .. } => Bitfield::complete(*size),
        }
    }

    fn size(&self) -> u64 {
        match self {
            Self::Empty => 0,
            Self::Complete { size, .. } => *size,
        }
    }
}

/// The bytes of a blob, for `bao-tree` to read as it encodes.
struct DataReader(Source);

impl ReadBytesAt for DataReader {
    fn read_bytes_at(&self, offset: u64, size: usize) -> io::Result<Bytes> {
        match &self.0 {
            Source::Empty => Ok(Bytes::new()),
            Source::Complete {
                db, id, size: len, ..
            } => {
                // Never more than is there; asking past the end finds nothing.
                let want = (*len).saturating_sub(offset).min(size as u64) as usize;
                let mut buf = vec![0u8; want];
                let connection = db.lock();
                let blob = connection
                    .blob_open("main", "blob", "data", *id, true)
                    .map_err(io::Error::other)?;
                blob.read_at_exact(&mut buf, offset as usize)
                    .map_err(io::Error::other)?;
                Ok(buf.into())
            }
        }
    }
}

/// At most `size` bytes of `data` from `offset`: less at the end, and none
/// after it.
fn slice_of(data: &[u8], offset: u64, size: usize) -> Bytes {
    let start = usize::try_from(offset).map_or(data.len(), |o| o.min(data.len()));
    let end = start.saturating_add(size).min(data.len());
    Bytes::copy_from_slice(&data[start..end])
}

/// The hashes that prove parts of a blob, read as `bao-tree` walks the tree.
struct OutboardReader {
    hash: blake3::Hash,
    tree: BaoTree,
    source: Source,
}

impl Outboard for OutboardReader {
    fn root(&self) -> blake3::Hash {
        self.hash
    }

    fn tree(&self) -> BaoTree {
        self.tree
    }

    fn load(&self, node: TreeNode) -> io::Result<Option<(blake3::Hash, blake3::Hash)>> {
        let Some(offset) = self.tree.pre_order_offset(node) else {
            return Ok(None);
        };
        let pair = match &self.source {
            Source::Empty => return Ok(None),
            Source::Complete { outboard, .. } => slice_of(outboard, offset * 64, 64),
        };
        if pair.len() != 64 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "short read"));
        }
        let left: [u8; 32] = pair[..32].try_into().expect("32 bytes");
        let right: [u8; 32] = pair[32..].try_into().expect("32 bytes");
        Ok(Some((left.into(), right.into())))
    }
}

/// Lets `bao-tree` send what it encodes down the protocol's channel.
struct EncodedSender<'a>(&'a mut mpsc::Sender<EncodedItem>);

impl bao_tree::io::mixed::Sender for EncodedSender<'_> {
    type Error = irpc::channel::SendError;
    async fn send(&mut self, item: EncodedItem) -> Result<(), Self::Error> {
        self.0.send(item).await
    }
}

// ---------------------------------------------------------------------
// The actor

struct Actor {
    commands: tokio::sync::mpsc::Receiver<Command>,
    shared: Arc<Shared>,
    tasks: JoinSet<()>,
    idle: Vec<oneshot::Sender<()>>,
}

impl Actor {
    fn spawn(&mut self, f: impl Future<Output = ()> + Send + 'static) {
        self.tasks.spawn(f);
    }

    async fn run(mut self) {
        let shutdown = loop {
            tokio::select! {
                command = self.commands.recv() => {
                    let Some(command) = command else { break None };
                    if let Some(shutdown) = self.handle(command).await {
                        break Some(shutdown);
                    }
                }
                Some(done) = self.tasks.join_next(), if !self.tasks.is_empty() => {
                    match done {
                        Ok(()) => {}
                        Err(e) if e.is_cancelled() => trace!("task cancelled"),
                        Err(e) => error!(error = %e, "blob task failed"),
                    }
                    if self.tasks.is_empty() {
                        for waiter in self.idle.drain(..) {
                            waiter.send(()).await.ok();
                        }
                    }
                }
            }
        };
        // Whatever is still being written finishes: it is one transaction.
        self.tasks.abort_all();
        if let Some(ShutdownMsg { tx, .. }) = shutdown {
            tx.send(()).await.ok();
        }
    }

    async fn handle(&mut self, command: Command) -> Option<ShutdownMsg> {
        let shared = self.shared.clone();
        match command {
            Command::ImportBytes(ImportBytesMsg {
                inner: ImportBytesRequest { data, format, .. },
                tx,
                ..
            }) => self.spawn(async move {
                import_bytes(&shared, data, format, tx).await;
            }),
            Command::ImportByteStream(ImportByteStreamMsg { tx, .. }) => self.spawn(async move {
                tx.send(AddProgressItem::Error(unsupported("importing a stream")))
                    .await
                    .ok();
            }),
            Command::ImportPath(ImportPathMsg { tx, .. }) => self.spawn(async move {
                tx.send(AddProgressItem::Error(unsupported("importing a path")))
                    .await
                    .ok();
            }),
            Command::ImportBao(ImportBaoMsg { tx, .. }) => self.spawn(async move {
                let e = api::Error::io(
                    io::ErrorKind::Unsupported,
                    unsupported("taking in a blob piece by piece").to_string(),
                );
                tx.send(Err(e)).await.ok();
            }),
            Command::ExportBao(ExportBaoMsg {
                inner: ExportBaoRequest { hash, ranges },
                mut tx,
                ..
            }) => self.spawn(async move {
                export_bao(&shared, hash, ranges, &mut tx).await;
            }),
            Command::ExportRanges(ExportRangesMsg { inner, mut tx, .. }) => {
                self.spawn(async move {
                    if let Err(e) = export_ranges(&shared, inner, &mut tx).await {
                        tx.send(ExportRangesItem::Error(e.into())).await.ok();
                    }
                });
            }
            Command::ExportPath(ExportPathMsg { tx, .. }) => self.spawn(async move {
                let e = api::Error::io(
                    io::ErrorKind::Unsupported,
                    unsupported("exporting to a path").to_string(),
                );
                tx.send(ExportProgressItem::Error(e)).await.ok();
            }),
            Command::Observe(ObserveMsg {
                inner: ObserveRequest { hash },
                tx,
                ..
            }) => self.spawn(async move {
                // What is here is all of it. A blob that isn't ends the stream
                // with nothing said, which the client reports.
                if let Ok(Some(source)) = shared.source(hash).await
                    && tx.send(source.bitfield()).await.is_ok()
                {
                    tx.closed().await;
                }
            }),
            Command::BlobStatus(BlobStatusMsg {
                inner: BlobStatusRequest { hash },
                tx,
                ..
            }) => self.spawn(async move {
                let status = match shared.source(hash).await {
                    Ok(None) | Err(_) => BlobStatus::NotFound,
                    Ok(Some(source)) => {
                        let bitfield = source.bitfield();
                        if bitfield.is_complete() {
                            BlobStatus::Complete {
                                size: bitfield.size(),
                            }
                        } else {
                            BlobStatus::Partial {
                                size: bitfield.validated_size(),
                            }
                        }
                    }
                };
                tx.send(status).await.ok();
            }),
            Command::ListBlobs(ListBlobsMsg { tx, .. }) => self.spawn(async move {
                let hashes = shared
                    .db
                    .with(|c| {
                        c.prepare("SELECT hash FROM blob")?
                            .query_map([], |r| r.get::<_, [u8; 32]>(0))?
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .await;
                match hashes {
                    Ok(hashes) => {
                        for hash in hashes {
                            if tx.send(Ok(Hash::from(hash))).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        tx.send(Err(store_error(e))).await.ok();
                    }
                }
            }),
            Command::DeleteBlobs(DeleteBlobsMsg {
                inner: BlobDeleteRequest { hashes, .. },
                tx,
                ..
            }) => self.spawn(async move {
                // Only iroh's own collector sends this, and none is run.
                tx.send(shared.delete(hashes).await.map_err(store_error))
                    .await
                    .ok();
            }),

            // No tags are kept: see the module's notes.
            Command::SetTag(SetTagMsg { tx, .. }) => {
                tx.send(Ok(())).await.ok();
            }
            Command::CreateTag(CreateTagMsg { tx, .. }) => {
                let tag = Tag::auto(std::time::SystemTime::now(), |_| false);
                tx.send(Ok(tag)).await.ok();
            }
            Command::DeleteTags(DeleteTagsMsg { tx, .. }) => {
                tx.send(Ok(0)).await.ok();
            }
            Command::RenameTag(RenameTagMsg { tx, .. }) => {
                let e = api::Error::io(io::ErrorKind::NotFound, "no tags are kept".to_owned());
                tx.send(Err(e)).await.ok();
            }
            Command::ListTags(ListTagsMsg { tx, .. }) => {
                tx.send(Vec::new()).await.ok();
            }
            Command::CreateTempTag(CreateTempTagMsg { inner, tx, .. }) => {
                tx.send(TempTag::new(inner.value, None)).await.ok();
            }
            Command::ListTempTags(msg) => {
                msg.tx.send(Vec::new()).await.ok();
            }

            // Scopes only matter to what collects garbage for iroh, and
            // nothing does, so every batch is in the global one.
            Command::Batch(BatchMsg { tx, mut rx, .. }) => self.spawn(async move {
                if tx.send(Scope::GLOBAL).await.is_ok() {
                    // Nothing is counted, so there is nothing to do as temp tags
                    // are dropped: just keep the scope until the client is done.
                    while let Ok(Some(BatchResponse::Drop(_) | BatchResponse::Ping)) =
                        rx.recv().await
                    {}
                }
            }),
            Command::ClearProtected(msg) => {
                msg.tx.send(Ok(())).await.ok();
            }
            Command::SyncDb(SyncDbMsg { tx, .. }) => {
                // Every write is its own committed transaction.
                tx.send(Ok(())).await.ok();
            }
            Command::WaitIdle(WaitIdleMsg { tx, .. }) => {
                if self.tasks.is_empty() {
                    tx.send(()).await.ok();
                } else {
                    self.idle.push(tx);
                }
            }
            Command::Shutdown(shutdown) => return Some(shutdown),
        }
        None
    }
}

// ---------------------------------------------------------------------
// Importing

/// Keeps a blob that is wholly here: its hash, its outboard and its data.
fn persist(
    connection: &mut Connection,
    hash: Hash,
    outboard: &[u8],
    data: &[u8],
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT OR IGNORE INTO blob (hash, size, outboard, data) VALUES (?1, ?2, ?3, ?4)",
        params![
            hash.as_bytes().as_slice(),
            data.len() as i64,
            outboard,
            data
        ],
    )?;
    Ok(())
}

async fn import_bytes(
    shared: &Shared,
    data: Bytes,
    format: BlobFormat,
    tx: mpsc::Sender<AddProgressItem>,
) {
    let size = data.len() as u64;
    if size > shared.max_blob {
        let e = io::Error::new(io::ErrorKind::InvalidInput, "the file is too large to keep");
        tx.send(AddProgressItem::Error(e)).await.ok();
        return;
    }
    // Whoever stopped listening for progress still wants the blob kept.
    tx.send(AddProgressItem::Size(size)).await.ok();
    tx.send(AddProgressItem::CopyDone).await.ok();
    // Hashing a large file is work for a blocking thread, not the runtime.
    let hashed = {
        let data = data.clone();
        tokio::task::spawn_blocking(move || {
            let outboard = PreOrderMemOutboard::create(&data, IROH_BLOCK_SIZE);
            (Hash::from(outboard.root()), outboard.data)
        })
        .await
    };
    let (hash, outboard) = match hashed {
        Ok(hashed) => hashed,
        Err(e) => {
            tx.send(AddProgressItem::Error(io::Error::other(e)))
                .await
                .ok();
            return;
        }
    };
    if hash != Hash::EMPTY {
        let stored = shared
            .db
            .with(move |c| persist(c, hash, &outboard, &data))
            .await;
        if let Err(e) = stored {
            tx.send(AddProgressItem::Error(e)).await.ok();
            return;
        }
    }
    let temp = TempTag::new(HashAndFormat { hash, format }, None);
    tx.send(AddProgressItem::Done(temp)).await.ok();
}

// ---------------------------------------------------------------------
// Exporting

async fn export_bao(
    shared: &Shared,
    hash: Hash,
    ranges: ChunkRanges,
    tx: &mut mpsc::Sender<EncodedItem>,
) {
    let not_found = || EncodeError::Io(io::Error::new(io::ErrorKind::NotFound, "hash not found"));
    let source = match shared.source(hash).await {
        Ok(Some(source)) => source,
        Ok(None) => {
            tx.send(not_found().into()).await.ok();
            return;
        }
        Err(e) => {
            tx.send(EncodeError::Io(e).into()).await.ok();
            return;
        }
    };
    let outboard = OutboardReader {
        hash: hash.into(),
        tree: BaoTree::new(source.size(), IROH_BLOCK_SIZE),
        source: source.clone(),
    };
    let mut sender = EncodedSender(tx);
    traverse_ranges_validated(DataReader(source), outboard, &ranges, &mut sender)
        .await
        .ok();
}

async fn export_ranges(
    shared: &Shared,
    request: ExportRangesRequest,
    tx: &mut mpsc::Sender<ExportRangesItem>,
) -> io::Result<()> {
    let ExportRangesRequest { ranges, hash } = request;
    let Some(source) = shared.source(hash).await? else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "hash not found"));
    };
    let bitfield = source.bitfield();
    let size = bitfield.size();
    let data = DataReader(source);
    tx.send(ExportRangesItem::Size(size)).await?;
    for range in ranges.iter() {
        let range = match range {
            RangeSetRange::Range(r) => size.min(*r.start)..size.min(*r.end),
            RangeSetRange::RangeFrom(r) => size.min(*r.start)..size,
        };
        let requested = ChunkRanges::bytes(range.start..range.end);
        if !bitfield.ranges.is_superset(&requested) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("missing range: {requested:?}, present: {bitfield:?}"),
            ));
        }
        // In pieces of a kilobyte, the last of what is left. A client that has
        // what it asked for stops listening, so the loop needs no more care
        // than to say where each piece is.
        for offset in (range.start..range.end).step_by(1024) {
            let end = (offset + 1024).min(range.end);
            let bytes = data.read_bytes_at(offset, (end - offset) as usize)?;
            tx.send(
                Leaf {
                    offset,
                    data: bytes,
                }
                .into(),
            )
            .await?;
        }
    }
    Ok(())
}
