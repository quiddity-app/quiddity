//! Blobs travelling between peers over real iroh connections on localhost.

use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use iroh::endpoint::presets;
use iroh::{Endpoint, EndpointAddr, RelayMode, SecretKey};
use quiddity_core::{Asset, Graph};
use quiddity_node::blobs::{self, Blobs, Hash};
use quiddity_node::{Allowed, Peer, Server};

const LOCAL: Option<SocketAddr> = Some(SocketAddr::new(
    std::net::IpAddr::V4(Ipv4Addr::LOCALHOST),
    0,
));

/// A peer on the network that keeps its blobs in `dir`, listing only the
/// devices the graph lists.
async fn device(graph: Graph, dir: &Path) -> (Peer, Server, Blobs) {
    let key = SecretKey::generate();
    let peer = Peer::new(
        graph,
        Allowed::Only(HashSet::new()),
        key.public().to_string(),
    );
    let blobs = Blobs::open(dir).await.unwrap();
    peer.set_blobs(blobs.clone());
    let server = quiddity_node::serve(&peer, key, LOCAL).await.unwrap();
    (peer, server, blobs)
}

fn address(server: &Server) -> EndpointAddr {
    EndpointAddr::new(server.endpoint().id()).with_ip_addr(server.endpoint().bound_sockets()[0])
}

/// Adds an entity to the graph that says it is a file with these bytes.
fn describe(peer: &Peer, name: &str, bytes: &[u8]) -> Hash {
    let hash = Hash::new(bytes);
    let asset = Asset {
        hash: hash.to_string(),
        mime: "application/octet-stream".into(),
        size: bytes.len() as u64,
        name: Some(name.into()),
    };
    let entity = peer.graph().create_block(None, None, name).unwrap();
    peer.graph()
        .create_block(Some(entity), None, &asset.line())
        .unwrap();
    peer.graph().commit();
    peer.changed();
    hash
}

async fn has_eventually(blobs: &Blobs, hash: Hash) -> bool {
    let wait = async {
        while !blobs.has(hash).await.unwrap() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .is_ok()
}

async fn join(guest: &Peer, guest_server: &Server, home: &Peer, home_server: &Server) {
    // Each device has to add the other, as when pairing.
    home.allow(&guest_server.endpoint().id().to_string())
        .unwrap();
    guest
        .admit(&home_server.endpoint().id().to_string())
        .unwrap();
    let idle = Some(Duration::from_secs(2));
    let (guest, endpoint, to) = (
        guest.clone(),
        guest_server.endpoint().clone(),
        address(home_server),
    );
    tokio::spawn(async move { guest.connect(&endpoint, to, idle).await });
}

/// A device that is told of a file by the graph gets its bytes from the
/// device it is connected to, byte for byte.
#[tokio::test]
async fn a_blob_follows_the_graph_that_names_it() {
    let (home_dir, guest_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (home, home_server, home_blobs) = device(Graph::new(), home_dir.path()).await;
    let bytes: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
    let stored = home_blobs.put(bytes.clone()).await.unwrap();
    assert_eq!(describe(&home, "data.bin", &bytes), stored);

    let (guest, guest_server, guest_blobs) = device(Graph::blank(), guest_dir.path()).await;
    assert!(!guest_blobs.has(stored).await.unwrap());
    join(&guest, &guest_server, &home, &home_server).await;

    assert!(
        has_eventually(&guest_blobs, stored).await,
        "the blob arrives"
    );
    assert_eq!(guest_blobs.get(stored).await.unwrap().to_vec(), bytes);
    assert!(guest.missing_blobs(&guest_blobs).await.is_empty());

    guest_server.shutdown().await.unwrap();
    home_server.shutdown().await.unwrap();
}

/// The blob a device is told of after it has connected comes too, and the
/// device that has the bytes may be the one that dialled.
#[tokio::test]
async fn a_blob_named_later_comes_from_whichever_device_has_it() {
    let (home_dir, guest_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (home, home_server, home_blobs) = device(Graph::new(), home_dir.path()).await;
    describe(&home, "first", b"first");
    home_blobs.put(&b"first"[..]).await.unwrap();
    let (guest, guest_server, guest_blobs) = device(Graph::blank(), guest_dir.path()).await;
    join(&guest, &guest_server, &home, &home_server).await;
    assert!(has_eventually(&guest_blobs, Hash::new(b"first")).await);

    // The guest dialled; now it is the guest that has the bytes.
    guest_blobs.put(&b"second"[..]).await.unwrap();
    let second = describe(&guest, "second", b"second");
    assert!(
        has_eventually(&home_blobs, second).await,
        "the dialler's blob"
    );

    guest_server.shutdown().await.unwrap();
    home_server.shutdown().await.unwrap();
}

/// Only the devices the graph lists may fetch: a stranger who knows the
/// hash and the address is refused.
#[tokio::test]
async fn a_device_the_graph_does_not_list_cannot_fetch() {
    let (home_dir, stranger_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (_home, home_server, home_blobs) = device(Graph::new(), home_dir.path()).await;
    let hash = home_blobs.put(&b"private"[..]).await.unwrap();

    let endpoint = Endpoint::builder(presets::Minimal)
        .relay_mode(RelayMode::Disabled)
        .bind_addr(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .unwrap()
        .bind()
        .await
        .unwrap();
    let stranger = Blobs::open(stranger_dir.path()).await.unwrap();
    let connection = endpoint
        .connect(address(&home_server), blobs::ALPN)
        .await
        .unwrap();
    let refused = stranger.fetch(connection, hash).await;
    assert!(refused.is_err(), "refused: {refused:?}");
    assert!(!stranger.has(hash).await.unwrap());

    home_server.shutdown().await.unwrap();
}

/// Only what the graph names and the store lacks is looked for.
#[tokio::test]
async fn only_missing_blobs_are_wanted() {
    let dir = tempfile::tempdir().unwrap();
    let (peer, server, blobs) = device(Graph::new(), dir.path()).await;
    assert!(peer.missing_blobs(&blobs).await.is_empty(), "no assets");
    let held = describe(&peer, "held", b"held");
    let lacking = describe(&peer, "lacking", b"lacking");
    describe(&peer, "lacking again", b"lacking");
    blobs.put(&b"held"[..]).await.unwrap();

    assert_eq!(peer.missing_blobs(&blobs).await, [lacking]);
    assert_ne!(held, lacking);
    server.shutdown().await.unwrap();
}
