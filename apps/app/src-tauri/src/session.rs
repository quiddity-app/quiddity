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

use quiddity_node::store::Store;
use quiddity_node::{Allowed, Peer};
use serde::Serialize;

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
}

impl Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

#[derive(Debug)]
pub struct Session {
    store: Store,
    peer: Peer,
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
            Err(quiddity_node::StoreError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                store.init(false)?
            }
            Err(e) => return Err(e.into()),
        };
        let device = store.key()?.public().to_string();
        let peer = Peer::new(graph, Allowed::Only(HashSet::new()), device);
        Ok(Self { store, peer })
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

    /// The whole graph, for the UI to start its replica from.
    pub fn snapshot(&self) -> Result<Vec<u8>, Error> {
        Ok(self.peer.graph().snapshot()?)
    }

    /// Applies updates from the UI, saves, and sends them to the other
    /// devices. Updates that arrive out of order are held until what they
    /// depend on does.
    pub fn apply(&self, updates: &[u8]) -> Result<(), Error> {
        let graph = self.peer.graph();
        graph.import(updates)?;
        self.peer.tidy()?;
        self.store.save(graph)?;
        self.peer.changed();
        Ok(())
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
        assert!(path.join("graph.loro").exists());
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
        session.apply(&ui.updates_since(&before).unwrap()).unwrap();

        let reopened = Session::open(dir.path()).unwrap();
        let graph = Graph::from_snapshot(&reopened.snapshot().unwrap()).unwrap();
        assert_eq!(graph.entity_name(page).unwrap(), "Typed in the UI");
    }

    #[test]
    fn nonsense_is_refused_and_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::open(dir.path()).unwrap();
        let before = session.snapshot().unwrap();
        assert!(session.apply(b"not an update").is_err());
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

    /// What the UI pushes reaches a device that has been let in, as it
    /// happens.
    #[tokio::test]
    async fn the_uis_edits_reach_other_devices() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session::open(dir.path()).unwrap();
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
        let connection = tokio::spawn(async move { peer.connect(&connect_to, addr, None).await });
        wait_for("the guest to join", || {
            guest.graph().version() == session.peer().graph().version()
        })
        .await;

        let ui = Graph::from_snapshot(&session.snapshot().unwrap()).unwrap();
        let before = ui.version();
        let page = ui.create_entity("Typed in the UI", &[]).unwrap();
        ui.commit();
        session.apply(&ui.updates_since(&before).unwrap()).unwrap();
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
}
