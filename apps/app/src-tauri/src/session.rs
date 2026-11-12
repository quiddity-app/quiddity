//! The open graph on disk, and on the network.
//!
//! The UI holds its own replica and answers its own queries (see
//! `quiddity-wasm`). The shell's part is to hand over the graph at startup,
//! apply the updates the UI's edits make, keep the result safe, and share it
//! with the user's other devices: the graph lives in a [`Peer`], which
//! `lib.rs` puts on the network. It knows nothing about Tauri, so it can be
//! tested on its own; `lib.rs` exposes it as commands.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use quiddity_core::{Graph, Version};
use quiddity_node::store::Store;
use quiddity_node::{Allowed, Peer, SyncState};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Store(#[from] quiddity_node::StoreError),
    #[error("{0}")]
    Core(#[from] quiddity_core::Error),
    #[error("{0}")]
    Device(#[from] quiddity_node::AllowError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("expected the updates as raw bytes")]
    NotBytes,
    #[error("{0}")]
    Blob(#[from] quiddity_node::blobs::BlobError),
    #[error("this device keeps no binaries")]
    NoBlobs,
    #[error("the graph opened, so there is nothing to recover from")]
    NothingToRecover,
    #[error("{0:?} is not a hash")]
    NotAHash(String),
}

impl Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

/// What to do when the graph on disk can't be opened. The damaged files are
/// never deleted: starting again sets them aside first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Recovery {
    /// Try again as it is, for a file that has been put right meanwhile.
    Retry,
    /// Start with nothing, to be filled by syncing with another device that
    /// has the graph.
    Blank,
    /// Start a new graph of its own, which is not the same graph as the
    /// other devices' and won't merge with it.
    Fresh,
}

/// The open graph. Cloning shares it.
#[derive(Debug, Clone)]
pub struct Session {
    store: Store,
    peer: Peer,
    /// What the UI is known to have: the snapshot it started from, what it
    /// has pushed, and what has been sent it since. What the graph has beyond
    /// this is what the UI lacks.
    ui: Arc<Mutex<Version>>,
}

impl Session {
    /// Opens the graph in `dir`, creating an empty one if there is none.
    ///
    /// Nobody may connect yet except the devices the graph lists, and the
    /// ones [`Session::allow`] lets in.
    pub fn open(dir: &Path) -> Result<Self, Error> {
        let store = Store::new(dir);
        let graph = match store.load() {
            Ok(graph) => graph,
            Err(quiddity_node::StoreError::Missing(_)) => store.init(false)?,
            Err(e) => return Err(e.into()),
        };
        let device = store.key()?.public().to_string();
        let peer = Peer::new(graph, Allowed::Only(HashSet::new()), device);
        Ok(Self {
            store,
            peer,
            ui: Arc::default(),
        })
    }

    /// Opens the graph in `dir` after it couldn't be opened, as `how` says.
    pub fn recover(dir: &Path, how: Recovery) -> Result<Self, Error> {
        if how != Recovery::Retry {
            let store = Store::new(dir);
            store.set_aside()?;
            store.init(how == Recovery::Blank)?;
        }
        Self::open(dir)
    }

    /// Opens the store for the bytes of the graph's binaries and has the peer
    /// keep them: it gives them to the devices that may connect, and fetches
    /// those the graph names that it lacks, once it is on the network. Call it
    /// before putting the peer on the network.
    pub async fn keep_blobs(&self) -> Result<(), Error> {
        let blobs = quiddity_node::blobs::Blobs::open(self.store.dir()).await?;
        self.peer.set_blobs(blobs);
        Ok(())
    }

    fn blobs(&self) -> Result<quiddity_node::blobs::Blobs, Error> {
        self.peer.blobs().ok_or(Error::NoBlobs)
    }

    /// Keeps the bytes of a file and returns their hash, in hex, which is
    /// what the graph refers to them by. The UI then says in the graph what
    /// the file is, as an edit like any other.
    pub async fn put_blob(&self, bytes: Vec<u8>) -> Result<String, Error> {
        Ok(self.blobs()?.put(bytes).await?.to_string())
    }

    /// The bytes with this hash, if this device has them (yet).
    pub async fn blob(&self, hash: &str) -> Result<Vec<u8>, Error> {
        let hash = hash.parse().map_err(|_| Error::NotAHash(hash.to_owned()))?;
        Ok(self.blobs()?.get(hash).await?.to_vec())
    }

    /// This device's ID, which another device needs to let it in.
    pub fn device_id(&self) -> Result<String, Error> {
        Ok(self.store.key()?.public().to_string())
    }

    /// The peer that shares the graph, for putting on the network.
    pub fn peer(&self) -> &Peer {
        &self.peer
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    fn ui(&self) -> std::sync::MutexGuard<'_, Version> {
        self.ui.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The whole graph, for the UI to start its replica from. A UI that asks
    /// again has started over, so what it has is this and nothing else.
    pub fn snapshot(&self) -> Result<Vec<u8>, Error> {
        let snapshot = self.peer.graph().snapshot()?;
        *self.ui() = Graph::version_of_updates(&snapshot)?;
        Ok(snapshot)
    }

    /// What the graph has changed to that the UI has not been told of: edits
    /// from other devices, and the shell's own (merging duplicates, say).
    /// None if there is nothing. What it returns, the UI is taken to have.
    pub fn ui_updates(&self) -> Result<Option<Vec<u8>>, Error> {
        let graph = self.peer.graph();
        let mut ui = self.ui();
        let now = graph.version();
        if ui.includes(&now) {
            return Ok(None);
        }
        // A graph that has been compacted past what the UI has can't update
        // it, and the UI is blank then: it is given the graph whole.
        let updates = if graph.can_update(&ui) {
            graph.updates_since(&ui)?
        } else {
            graph.snapshot()?
        };
        ui.merge(&now);
        Ok(Some(updates))
    }

    /// Applies updates from the UI, saves, and sends them to the other
    /// devices. Updates that arrive out of order are held until what they
    /// depend on does.
    ///
    /// `typed` are the blocks the UI says it was typing in, and nothing else.
    /// Those edits are sent only to a device that has the block open, or once
    /// the typing is over (see `Peer::edited`). With none, whatever changed is
    /// sent at once.
    pub fn apply(&self, updates: &[u8], typed: &[String]) -> Result<(), Error> {
        let graph = self.peer.graph();
        graph.import(updates)?;
        // The UI made them, so it has them.
        self.ui().merge(&Graph::version_of_updates(updates)?);
        self.peer.tidy()?;
        self.store.save(graph)?;
        if typed.is_empty() {
            self.peer.changed();
        }
        for block in typed {
            self.peer.edited(block.as_str());
        }
        Ok(())
    }

    /// Says which blocks the user has open, to the other devices. Moving off a
    /// block ends the typing in it.
    pub fn set_focus(&self, blocks: &[String]) {
        self.peer.set_focus(blocks.iter().map(String::as_str));
    }

    /// How this device stands with the others, as the UI names it: `alone`
    /// (no other device), `offline` (others, none connected), `behind`
    /// (connected, and either side is missing something), `synced`, or `present`
    /// (synced, and another device has a block open).
    pub fn sync_state(&self) -> &'static str {
        match self.peer.sync_state() {
            SyncState::Alone => "alone",
            SyncState::Offline => "offline",
            SyncState::Behind => "behind",
            SyncState::Synced => "synced",
            SyncState::Present => "present",
        }
    }

    /// Lets the device `device` connect: lists it in the graph, or, if this
    /// device has not joined a graph yet, trusts it until the app closes.
    pub fn allow(&self, device: &str) -> Result<(), Error> {
        Ok(self.peer.admit(device)?)
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddr};
    use std::time::Duration;

    use iroh::EndpointAddr;
    use quiddity_core::Graph;

    use super::*;

    #[test]
    fn creates_a_graph_when_there_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph");
        let session = Session::open(&path).unwrap();
        assert!(path.join("graph.quiddity").exists());
        Graph::from_snapshot(&session.snapshot().unwrap()).unwrap();
    }

    /// The UI's side of it: start from the snapshot, edit, send the updates.
    #[test]
    fn the_uis_edits_are_applied_and_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::open(dir.path()).unwrap();
        let ui = Graph::from_snapshot(&session.snapshot().unwrap()).unwrap();
        let before = ui.version();
        let page = ui.create_entity("Typed in the UI", &[]).unwrap();
        ui.commit();
        session
            .apply(&ui.updates_since(&before).unwrap(), &[])
            .unwrap();

        let reopened = Session::open(dir.path()).unwrap();
        let graph = Graph::from_snapshot(&reopened.snapshot().unwrap()).unwrap();
        assert_eq!(graph.entity_name(page).unwrap(), "Typed in the UI");
    }

    #[test]
    fn nonsense_is_refused_and_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::open(dir.path()).unwrap();
        let before = session.snapshot().unwrap();
        assert!(session.apply(b"not an update", &[]).is_err());
        assert_eq!(session.snapshot().unwrap(), before);
    }

    fn local() -> Option<SocketAddr> {
        Some(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
    }

    /// A second device, blank, that can dial the session's endpoint.
    async fn guest(server: &quiddity_node::Server) -> (Peer, iroh::Endpoint, EndpointAddr) {
        let key = iroh::SecretKey::generate();
        let guest = Peer::new(
            Graph::blank(),
            Allowed::Only(HashSet::from([server.endpoint().id()])),
            key.public().to_string(),
        );
        let endpoint = quiddity_node::bind(key, local()).await.unwrap();
        let addr = EndpointAddr::new(server.endpoint().id())
            .with_ip_addr(server.endpoint().bound_sockets()[0]);
        (guest, endpoint, addr)
    }

    /// A session on the network, and a second device let in and connected,
    /// having caught up.
    async fn joined(
        session: &Session,
    ) -> (quiddity_node::Server, Peer, tokio::task::JoinHandle<()>) {
        let server = quiddity_node::serve(session.peer(), session.store().key().unwrap(), local())
            .await
            .unwrap();
        let (guest, endpoint, addr) = guest(&server).await;

        // Nobody has let the guest in.
        let refused = guest
            .connect(&endpoint, addr.clone(), Some(Duration::from_secs(2)))
            .await;
        assert!(refused.is_err());

        session.allow(&guest_id(&endpoint)).unwrap();
        let (peer, connect_to) = (guest.clone(), endpoint.clone());
        let connection = tokio::spawn(async move {
            let _ = peer.connect(&connect_to, addr, None).await;
        });
        wait_for("the guest to join", || {
            guest.graph().version() == session.peer().graph().version()
                && !session.peer().present().is_empty()
        })
        .await;
        (server, guest, connection)
    }

    /// What the UI pushes reaches a device that has been let in, as it
    /// happens.
    #[tokio::test]
    async fn the_uis_edits_reach_other_devices() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::open(dir.path()).unwrap();
        let (server, guest, connection) = joined(&session).await;

        let ui = Graph::from_snapshot(&session.snapshot().unwrap()).unwrap();
        let before = ui.version();
        let page = ui.create_entity("Typed in the UI", &[]).unwrap();
        ui.commit();
        session
            .apply(&ui.updates_since(&before).unwrap(), &[])
            .unwrap();
        wait_for("the edit to arrive", || {
            guest
                .graph()
                .entity_name(page)
                .is_ok_and(|n| n == "Typed in the UI")
        })
        .await;

        connection.abort();
        server.shutdown().await.unwrap();
    }

    /// An edit pushed as typing in a block nobody else has open waits; it goes
    /// when the user moves on, or at once if someone has the block open.
    #[tokio::test]
    async fn typing_is_held_back_until_it_is_seen_or_over() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::open(dir.path()).unwrap();
        session.peer().set_edit_idle(Duration::from_secs(60));
        let (server, guest, connection) = joined(&session).await;
        let ui = Graph::from_snapshot(&session.snapshot().unwrap()).unwrap();

        let type_in = |name: &str, block: &str| {
            let before = ui.version();
            let page = ui.create_entity(name, &[]).unwrap();
            ui.commit();
            let updates = ui.updates_since(&before).unwrap();
            session.apply(&updates, &[block.to_owned()]).unwrap();
            page
        };
        let arrived = |page| guest.graph().entity_name(page).is_ok();

        // Typing in a block the user has open, and nobody else has: held.
        session.set_focus(&["a".to_owned()]);
        let first = type_in("Held", "a");
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(!arrived(first), "held back while typing");

        // Moving to another block ends it.
        session.set_focus(&["b".to_owned()]);
        wait_for("the typing to be sent", || arrived(first)).await;

        // A block someone else has open: sent as it is typed.
        guest.set_focus(["b"]);
        wait_for("the guest to be seen in the block", || {
            session.peer().watched("b")
        })
        .await;
        let second = type_in("Seen", "b");
        wait_for("the watched typing to arrive", || arrived(second)).await;

        connection.abort();
        server.shutdown().await.unwrap();
    }

    /// The blocks the user has open are what other devices see.
    #[tokio::test]
    async fn the_open_blocks_are_seen_by_other_devices() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::open(dir.path()).unwrap();
        let (server, guest, connection) = joined(&session).await;

        session.set_focus(&["a".to_owned(), "b".to_owned()]);
        wait_for("the guest to see them", || {
            guest.watched("a") && guest.watched("b")
        })
        .await;
        session.set_focus(&[]);
        wait_for("the guest to see none", || !guest.watched("a")).await;

        connection.abort();
        server.shutdown().await.unwrap();
    }

    fn guest_id(endpoint: &iroh::Endpoint) -> String {
        endpoint.id().to_string()
    }

    async fn wait_for(what: &str, condition: impl Fn() -> bool) {
        let wait = async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(10), wait)
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
    }

    /// A device that has joined a graph lists the others; one that hasn't
    /// has nothing to list them in, so trusts them for now. Either way the ID
    /// has to be one.
    #[test]
    fn allowing_a_device_lists_it_or_trusts_it() {
        let id = iroh::SecretKey::generate().public().to_string();

        let joined = Session::open(tempfile::tempdir().unwrap().path()).unwrap();
        joined.allow(&id).unwrap();
        assert!(joined.peer().graph().devices().iter().any(|d| d.id == id));
        assert!(matches!(
            joined.allow("not a device"),
            Err(Error::Device(quiddity_node::AllowError::NotADevice(_)))
        ));

        let dir = tempfile::tempdir().unwrap();
        Store::new(dir.path()).init(true).unwrap();
        let blank = Session::open(dir.path()).unwrap();
        blank.allow(&id).unwrap();
        assert!(blank.peer().graph().devices().is_empty());
    }

    /// A graph that can't be read stops the session opening, and each way of
    /// going on does what it says without deleting what is there.
    #[test]
    fn a_damaged_graph_can_be_recovered_from() {
        let damaged = |dir: &Path| {
            Store::new(dir).init(false).unwrap();
            std::fs::write(dir.join("graph.quiddity"), b"not a snapshot").unwrap();
        };
        let kept = |dir: &Path| {
            std::fs::read_dir(dir)
                .unwrap()
                .filter(|e| {
                    let name = e.as_ref().unwrap().file_name();
                    name.to_string_lossy()
                        .starts_with("graph.quiddity.damaged-")
                })
                .count()
        };

        let dir = tempfile::tempdir().unwrap();
        damaged(dir.path());
        assert!(Session::open(dir.path()).is_err());
        assert!(Session::recover(dir.path(), Recovery::Retry).is_err());
        assert_eq!(kept(dir.path()), 0, "retrying changes nothing");

        let blank = Session::recover(dir.path(), Recovery::Blank).unwrap();
        assert_eq!(blank.peer().graph().version(), Version::default());
        assert_eq!(kept(dir.path()), 1);
        assert_eq!(
            std::fs::read(
                std::fs::read_dir(dir.path())
                    .unwrap()
                    .map(|e| e.unwrap().path())
                    .find(|p| p.to_string_lossy().contains(".damaged-"))
                    .unwrap()
            )
            .unwrap(),
            b"not a snapshot"
        );

        let dir = tempfile::tempdir().unwrap();
        damaged(dir.path());
        let fresh = Session::recover(dir.path(), Recovery::Fresh).unwrap();
        assert_ne!(fresh.peer().graph().version(), Version::default());
        assert_eq!(kept(dir.path()), 1);
        // And it opens as any graph does now.
        Session::open(dir.path()).unwrap();
    }

    #[test]
    fn the_device_id_is_the_endpoint_id() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::open(dir.path()).unwrap();
        let key = session.store().key().unwrap();
        assert_eq!(session.device_id().unwrap(), key.public().to_string());
        // The same on the next start: the key is kept.
        let again = Session::open(dir.path()).unwrap();
        assert_eq!(again.device_id().unwrap(), session.device_id().unwrap());
    }

    /// Starts a UI replica from the session, as the app does.
    fn ui_replica(session: &Session) -> Graph {
        Graph::from_snapshot(&session.snapshot().unwrap()).unwrap()
    }

    /// Another device's edit, as it arrives at the session's graph.
    fn from_elsewhere(session: &Session, name: &str) -> quiddity_core::BlockId {
        let other = Graph::from_snapshot(&session.snapshot().unwrap()).unwrap();
        let before = other.version();
        let page = other.create_entity(name, &[]).unwrap();
        other.commit();
        session
            .peer()
            .graph()
            .import(&other.updates_since(&before).unwrap())
            .unwrap();
        page
    }

    /// Changes that come from other devices are sent to the UI once, and
    /// leave it with the same graph.
    #[test]
    fn other_devices_changes_reach_the_ui_once() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::open(dir.path()).unwrap();
        let ui = ui_replica(&session);
        assert_eq!(session.ui_updates().unwrap(), None, "nothing yet");

        let page = from_elsewhere(&session, "From another device");
        let updates = session.ui_updates().unwrap().expect("something to send");
        ui.import(&updates).unwrap();
        assert_eq!(ui.entity_name(page).unwrap(), "From another device");
        assert_eq!(ui.version(), session.peer().graph().version());
        assert_eq!(session.ui_updates().unwrap(), None, "and only once");
    }

    /// A device that joins a compacted graph holds it from a snapshot, which
    /// the UI, still blank, must be given whole: updates from nothing would
    /// never apply.
    #[test]
    fn a_blank_ui_is_given_a_compacted_graph_whole() {
        let dir = tempfile::tempdir().unwrap();
        Store::new(dir.path()).init(true).unwrap();
        let session = Session::open(dir.path()).unwrap();
        let ui = ui_replica(&session);
        assert_eq!(ui.version(), Version::default());

        let source = Graph::new();
        source.create_block(None, None, "before").unwrap();
        source.commit();
        let at = source.version();
        source.create_block(None, None, "after").unwrap();
        source.commit();
        let compacted = Graph::from_snapshot(&source.shallow_snapshot(&at).unwrap()).unwrap();
        session
            .peer()
            .graph()
            .import(&compacted.snapshot().unwrap())
            .unwrap();

        ui.import(&session.ui_updates().unwrap().unwrap()).unwrap();
        assert_eq!(ui.version(), session.peer().graph().version());
        assert_eq!(session.ui_updates().unwrap(), None);
    }

    /// What the UI pushed is not sent back to it. Saving records the device
    /// in the graph, which the UI has not seen, so something small may follow
    /// a push; once the UI has it, there is nothing.
    #[test]
    fn the_uis_own_edits_do_not_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::open(dir.path()).unwrap();
        let ui = ui_replica(&session);
        let untouched = ui_replica(&session);
        let before = ui.version();
        let page = ui.create_entity("Typed in the UI", &[]).unwrap();
        ui.commit();
        session
            .apply(&ui.updates_since(&before).unwrap(), &[])
            .unwrap();

        if let Some(record) = session.ui_updates().unwrap() {
            // The record rests on the edit, so a replica that never saw the
            // edit can't take it: the edit is not in it.
            assert!(untouched.import(&record).is_err(), "the edit came back");
            ui.import(&record).unwrap();
        }
        assert_eq!(ui.version(), session.peer().graph().version());
        assert_eq!(session.ui_updates().unwrap(), None);
        assert_eq!(ui.entity_name(page).unwrap(), "Typed in the UI");

        // Nor does it hide what came from elsewhere in between.
        from_elsewhere(&session, "Meanwhile");
        assert!(session.ui_updates().unwrap().is_some());
    }

    /// A UI that starts over is given everything again, and told only what
    /// is newer than that.
    #[test]
    fn a_ui_that_starts_over_is_not_sent_what_it_has() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::open(dir.path()).unwrap();
        from_elsewhere(&session, "Before the reload");
        let fresh = ui_replica(&session);
        assert_eq!(session.ui_updates().unwrap(), None);

        let page = from_elsewhere(&session, "After");
        fresh
            .import(&session.ui_updates().unwrap().unwrap())
            .unwrap();
        assert_eq!(fresh.entity_name(page).unwrap(), "After");
    }

    /// The UI's word for how this device stands with the others, from nothing
    /// to listed to connected to seen.
    #[tokio::test]
    async fn the_sync_state_is_named_for_the_ui() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::open(dir.path()).unwrap();
        assert_eq!(session.sync_state(), "alone");

        let (server, guest, connection) = joined(&session).await;
        wait_for("them to be up to date", || session.sync_state() == "synced").await;

        guest.set_focus(["a"]);
        wait_for("the guest to be seen", || session.sync_state() == "present").await;

        connection.abort();
        server.shutdown().await.unwrap();
        wait_for("the connection to go", || session.sync_state() == "offline").await;
    }

    /// A file put in comes back out under the hash of its bytes, and what
    /// isn't there, or isn't a hash, is refused rather than made up.
    #[tokio::test]
    async fn a_file_is_kept_by_its_hash() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::open(dir.path()).unwrap();
        session.keep_blobs().await.unwrap();
        let bytes: Vec<u8> = (0..300_000u32).map(|i| (i % 253) as u8).collect();

        let hash = session.put_blob(bytes.clone()).await.unwrap();
        assert_eq!(hash, quiddity_node::blobs::Hash::new(&bytes).to_string());
        assert_eq!(session.put_blob(bytes.clone()).await.unwrap(), hash);
        assert_eq!(session.blob(&hash).await.unwrap(), bytes);

        let absent = quiddity_node::blobs::Hash::new(b"absent").to_string();
        assert!(matches!(session.blob(&absent).await, Err(Error::Blob(_))));
        assert!(matches!(
            session.blob("not a hash").await,
            Err(Error::NotAHash(_))
        ));
    }

    /// A device whose store couldn't be opened still runs, and says so.
    #[tokio::test]
    async fn without_a_store_files_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::open(dir.path()).unwrap();
        assert!(matches!(
            session.put_blob(b"x".to_vec()).await,
            Err(Error::NoBlobs)
        ));
        let hash = quiddity_node::blobs::Hash::new(b"x").to_string();
        assert!(matches!(session.blob(&hash).await, Err(Error::NoBlobs)));
    }

    /// The whole of it: a file put in the shell and named by the UI's edit
    /// reaches another device that keeps blobs, with the graph that names it.
    #[tokio::test]
    async fn a_file_the_ui_names_reaches_other_devices() {
        use quiddity_node::blobs::Blobs;

        let (home_dir, guest_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let home = Session::open(home_dir.path()).unwrap();
        home.keep_blobs().await.unwrap();
        let bytes: Vec<u8> = (0..200_000u32).map(|i| (i % 249) as u8).collect();
        let hash = home.put_blob(bytes.clone()).await.unwrap();

        // The UI says in the graph what the file is, as an edit like any other.
        let ui = Graph::from_snapshot(&home.snapshot().unwrap()).unwrap();
        let before = ui.version();
        let file = ui.create_block(None, None, "data.bin").unwrap();
        let asset = quiddity_core::Asset {
            hash: hash.clone(),
            mime: "application/octet-stream".into(),
            size: bytes.len() as u64,
            name: Some("data.bin".into()),
        };
        ui.create_block(Some(file), None, &asset.line()).unwrap();
        ui.commit();
        home.apply(&ui.updates_since(&before).unwrap(), &[])
            .unwrap();

        let server = quiddity_node::serve(home.peer(), home.store().key().unwrap(), local())
            .await
            .unwrap();
        let key = iroh::SecretKey::generate();
        let guest = Peer::new(
            Graph::blank(),
            Allowed::Only(HashSet::new()),
            key.public().to_string(),
        );
        let guest_blobs = Blobs::open(guest_dir.path()).await.unwrap();
        guest.set_blobs(guest_blobs.clone());
        let guest_server = quiddity_node::serve(&guest, key, local()).await.unwrap();

        // Each device lists the other, as when pairing.
        home.allow(&guest_server.endpoint().id().to_string())
            .unwrap();
        guest.admit(&server.endpoint().id().to_string()).unwrap();
        let to = EndpointAddr::new(server.endpoint().id())
            .with_ip_addr(server.endpoint().bound_sockets()[0]);
        let (peer, endpoint) = (guest.clone(), guest_server.endpoint().clone());
        let connection = tokio::spawn(async move {
            let _ = peer.connect(&endpoint, to, None).await;
        });

        let hash: quiddity_node::blobs::Hash = hash.parse().unwrap();
        let arrives = async {
            while !guest_blobs.has(hash).await.unwrap() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(10), arrives)
            .await
            .expect("the file reaches the other device");
        assert_eq!(guest_blobs.get(hash).await.unwrap().to_vec(), bytes);

        connection.abort();
        server.shutdown().await.unwrap();
        guest_server.shutdown().await.unwrap();
    }
}
