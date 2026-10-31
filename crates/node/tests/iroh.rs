//! Peers syncing over real iroh connections on localhost.

use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use iroh::endpoint::{Connection, presets};
use iroh::protocol::Router;
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh::{Endpoint, EndpointAddr, RelayMode};
use quiddity_core::{BlockId, Graph, Index, ensure_journal};
use quiddity_node::{ALPN, Allowed, Peer};

/// A peer listening on localhost only: no relays, no address lookup.
async fn listen(peer: &Peer) -> (Router, EndpointAddr) {
    let endpoint = Endpoint::builder(presets::Minimal)
        .relay_mode(RelayMode::Disabled)
        .bind_addr(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .unwrap()
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .unwrap();
    let addr = EndpointAddr::new(endpoint.id()).with_ip_addr(endpoint.bound_sockets()[0]);
    let router = Router::builder(endpoint).accept(ALPN, peer.clone()).spawn();
    (router, addr)
}

fn layout(graph: &Graph) -> Vec<(BlockId, Option<BlockId>, String)> {
    graph
        .blocks()
        .into_iter()
        .map(|b| (b, graph.parent(b), graph.text(b).unwrap()))
        .collect()
}

/// Waits until all the peers' graphs match.
async fn converged(peers: &[&Peer]) {
    let wait = async {
        loop {
            let first = layout(peers[0].graph());
            if peers.iter().all(|p| layout(p.graph()) == first) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .expect("peers converge");
}

fn edit(peer: &Peer, text: &str) {
    peer.graph().create_block(None, None, text).unwrap();
    peer.graph().commit();
    peer.changed();
}

#[tokio::test]
async fn two_devices_sync_and_stay_in_sync() {
    let laptop = Peer::new(Graph::new(), Allowed::Any, "laptop");
    edit(&laptop, "before connecting");
    let phone = Peer::new(Graph::blank(), Allowed::Any, "phone");

    let (laptop_router, laptop_addr) = listen(&laptop).await;
    let (phone_router, _) = listen(&phone).await;
    let dialer = phone.clone();
    let endpoint = phone_router.endpoint().clone();
    let connection =
        tokio::spawn(async move { dialer.connect(&endpoint, laptop_addr, None).await });

    converged(&[&laptop, &phone]).await;
    // While connected, edits on either side arrive without reconnecting.
    edit(&phone, "from the phone");
    edit(&laptop, "from the laptop");
    converged(&[&laptop, &phone]).await;
    assert_eq!(laptop.graph().roots().len(), 3);

    connection.abort();
    laptop_router.shutdown().await.unwrap();
    phone_router.shutdown().await.unwrap();
}

#[tokio::test]
async fn devices_that_never_meet_sync_through_a_node() {
    let node = Peer::new(Graph::new(), Allowed::Any, "node");
    let phone = Peer::new(Graph::blank(), Allowed::Any, "phone");
    let laptop = Peer::new(Graph::blank(), Allowed::Any, "laptop");
    let (node_router, node_addr) = listen(&node).await;
    let (phone_router, _) = listen(&phone).await;
    let (laptop_router, _) = listen(&laptop).await;

    let mut tasks = Vec::new();
    for (peer, router) in [(&phone, &phone_router), (&laptop, &laptop_router)] {
        let (peer, endpoint, addr) = (peer.clone(), router.endpoint().clone(), node_addr.clone());
        tasks.push(tokio::spawn(async move {
            peer.connect(&endpoint, addr, None).await
        }));
    }
    converged(&[&node, &phone, &laptop]).await;
    edit(&phone, "written on the phone");
    edit(&laptop, "written on the laptop");
    converged(&[&node, &phone, &laptop]).await;
    assert_eq!(phone.graph().roots().len(), 2);

    tasks.iter().for_each(tokio::task::JoinHandle::abort);
    for router in [node_router, phone_router, laptop_router] {
        router.shutdown().await.unwrap();
    }
}

/// Two devices that each started the day's journal while apart: the node
/// merges them, and the devices (which don't merge for themselves here)
/// get the merge while they stay connected.
#[tokio::test]
async fn merges_reach_connected_devices() {
    let date = "2026-10-26".parse().unwrap();
    let node = Peer::new(Graph::new(), Allowed::Any, "node");
    let devices = [
        Peer::new(Graph::blank(), Allowed::Any, "device-0"),
        Peer::new(Graph::blank(), Allowed::Any, "device-1"),
    ];
    for (i, device) in devices.iter().enumerate() {
        let graph = device.graph();
        let journal = ensure_journal(graph, &Index::build(graph).unwrap(), date).unwrap();
        graph
            .create_block(Some(journal), None, &format!("from device {i}"))
            .unwrap();
        graph.commit();
    }

    let (node_router, node_addr) = listen(&node).await;
    let mut routers = Vec::new();
    let mut tasks = Vec::new();
    for device in &devices {
        let (router, _) = listen(device).await;
        let (device, endpoint, addr) =
            (device.clone(), router.endpoint().clone(), node_addr.clone());
        tasks.push(tokio::spawn(async move {
            device.connect(&endpoint, addr, None).await
        }));
        routers.push(router);
    }
    let all = [&node, &devices[0], &devices[1]];
    converged(&all).await;
    assert_eq!(Index::build(node.graph()).unwrap().duplicates().count(), 1);

    assert!(node.tidy().unwrap(), "the duplicate's entry moves");
    assert!(!node.tidy().unwrap(), "and then there's nothing to do");
    converged(&all).await;
    let graph = devices[0].graph();
    let journal = Index::build(graph).unwrap().journal(date).unwrap();
    let entries = graph.children(journal).len() - 1; // and the properties
    assert_eq!(entries, 2);

    tasks.iter().for_each(tokio::task::JoinHandle::abort);
    node_router.shutdown().await.unwrap();
    for router in routers {
        router.shutdown().await.unwrap();
    }
}

/// A device whose edits rest on history its peer has dropped (one that was
/// evicted) is told so, rather than that the connection went.
#[tokio::test]
async fn a_device_behind_its_peer_is_told_so() {
    let graph = Graph::new();
    let evicted = Graph::from_snapshot(&graph.snapshot().unwrap()).unwrap();
    graph.create_block(None, None, "later").unwrap();
    graph.commit();
    let compacted =
        Graph::from_snapshot(&graph.shallow_snapshot(&graph.version()).unwrap()).unwrap();
    evicted
        .create_block(None, None, "made on dropped history")
        .unwrap();
    evicted.commit();

    let node = Peer::new(compacted, Allowed::Any, "node");
    let device = Peer::new(evicted, Allowed::Any, "evicted");
    let (node_router, node_addr) = listen(&node).await;
    let (device_router, _) = listen(&device).await;
    let result = device
        .connect(
            device_router.endpoint(),
            node_addr,
            Some(Duration::from_secs(2)),
        )
        .await;
    assert!(
        matches!(result, Err(quiddity_node::Error::Behind)),
        "{result:?}"
    );

    node_router.shutdown().await.unwrap();
    device_router.shutdown().await.unwrap();
}

#[tokio::test]
async fn unlisted_devices_are_refused() {
    let node = Peer::new(Graph::new(), Allowed::Only(HashSet::new()), "node");
    edit(&node, "private");
    let (node_router, node_addr) = listen(&node).await;
    let stranger = Peer::new(Graph::blank(), Allowed::Any, "stranger");
    let (stranger_router, _) = listen(&stranger).await;

    let result = stranger
        .connect(
            stranger_router.endpoint(),
            node_addr,
            Some(Duration::from_secs(2)),
        )
        .await;
    assert!(result.is_err(), "the connection is closed on them");
    assert!(
        !matches!(result, Err(quiddity_node::Error::Behind)),
        "and not told it's behind"
    );
    assert!(stranger.graph().roots().is_empty());

    node_router.shutdown().await.unwrap();
    stranger_router.shutdown().await.unwrap();
}

/// Waits until `condition` holds.
async fn eventually(what: &str, condition: impl Fn() -> bool) {
    let wait = async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

/// Devices that only meet through a node still see each other's blocks, and
/// stop seeing them when the device goes, though it never says so.
#[tokio::test]
async fn presence_reaches_devices_through_a_node_and_goes_with_them() {
    let node = Peer::new(Graph::new(), Allowed::Any, "node");
    let phone = Peer::new(Graph::blank(), Allowed::Any, "phone");
    let laptop = Peer::new(Graph::blank(), Allowed::Any, "laptop");
    let (node_router, node_addr) = listen(&node).await;
    let (phone_router, _) = listen(&phone).await;
    let (laptop_router, _) = listen(&laptop).await;

    let mut tasks = Vec::new();
    for (peer, router) in [(&phone, &phone_router), (&laptop, &laptop_router)] {
        let (peer, endpoint, addr) = (peer.clone(), router.endpoint().clone(), node_addr.clone());
        tasks.push(tokio::spawn(async move {
            peer.connect(&endpoint, addr, None).await
        }));
    }

    phone.set_focus(["block-1"]);
    eventually("the node to see the phone's block", || {
        node.watched("block-1")
    })
    .await;
    eventually("the laptop to see it too", || laptop.watched("block-1")).await;
    assert!(!phone.watched("block-1"), "nobody else has it open");
    assert_eq!(laptop.present().len(), 2, "the node and the phone");

    phone.set_focus(["block-2"]);
    eventually("the move to arrive", || {
        laptop.watched("block-2") && !laptop.watched("block-1")
    })
    .await;

    // The phone vanishes; only the node notices.
    tasks[0].abort();
    phone_router.shutdown().await.unwrap();
    eventually("the laptop to forget the phone", || {
        !laptop.watched("block-2") && laptop.present().len() == 1
    })
    .await;

    tasks[1].abort();
    node_router.shutdown().await.unwrap();
    laptop_router.shutdown().await.unwrap();
}

/// Devices that dial each other at once have two connections. Presence
/// survives the loss of one.
#[tokio::test]
async fn presence_survives_one_of_two_connections_closing() {
    let a = Peer::new(Graph::new(), Allowed::Any, "a");
    let b = Peer::new(Graph::blank(), Allowed::Any, "b");
    let (a_router, a_addr) = listen(&a).await;
    let (b_router, b_addr) = listen(&b).await;

    let dial = |from: &Peer, router: &Router, to: EndpointAddr| {
        let (peer, endpoint) = (from.clone(), router.endpoint().clone());
        tokio::spawn(async move { peer.connect(&endpoint, to, None).await })
    };
    let first = dial(&b, &b_router, a_addr);
    let second = dial(&a, &a_router, b_addr);

    a.set_focus(["x"]);
    eventually("b to see a's block", || b.watched("x")).await;

    first.abort();
    // The first connection's end is noticed, and the second carries on:
    // a change made afterwards still arrives.
    a.set_focus(["y"]);
    eventually("the change to arrive", || b.watched("y") && !b.watched("x")).await;

    second.abort();
    a_router.shutdown().await.unwrap();
    b_router.shutdown().await.unwrap();
}

/// Two connected devices that have converged, with the phone's edits held
/// for `idle` unless something sooner sends them.
async fn connected_pair(idle: Duration) -> (Peer, Peer, Vec<Router>, tokio::task::JoinHandle<()>) {
    let laptop = Peer::new(Graph::new(), Allowed::Any, "laptop");
    let phone = Peer::new(Graph::blank(), Allowed::Any, "phone");
    phone.set_edit_idle(idle);
    let (laptop_router, laptop_addr) = listen(&laptop).await;
    let (phone_router, _) = listen(&phone).await;
    let (dialer, endpoint) = (phone.clone(), phone_router.endpoint().clone());
    let task = tokio::spawn(async move {
        let _ = dialer.connect(&endpoint, laptop_addr, None).await;
    });
    converged(&[&laptop, &phone]).await;
    eventually("the devices to see each other", || {
        phone.present().len() == 1 && laptop.present().len() == 1
    })
    .await;
    (laptop, phone, vec![laptop_router, phone_router], task)
}

/// Types into the phone, as the view will: committed, saved, and announced
/// as an edit.
fn type_into(phone: &Peer, block: &str, text: &str) {
    phone.graph().create_block(None, None, text).unwrap();
    phone.graph().commit();
    phone.edited(block);
}

async fn shut_down(routers: Vec<Router>, task: tokio::task::JoinHandle<()>) {
    task.abort();
    for router in routers {
        router.shutdown().await.unwrap();
    }
}

/// How long to give a held edit to (wrongly) turn up.
const SETTLE: Duration = Duration::from_millis(500);

#[tokio::test]
async fn edits_nobody_is_watching_wait_until_the_edit_is_over() {
    let (laptop, phone, routers, task) = connected_pair(Duration::from_millis(1500)).await;
    let saved = phone.subscribe();

    type_into(&phone, "blk", "typed");
    assert!(saved.has_changed().unwrap(), "saving isn't held back");
    tokio::time::sleep(SETTLE).await;
    assert!(laptop.graph().roots().is_empty(), "held back while typing");

    // More typing moves the deadline, which is the idle time since the
    // last: the first deadline (1.5 s) passes with the edit still held.
    tokio::time::sleep(Duration::from_millis(500)).await;
    type_into(&phone, "blk", "more");
    tokio::time::sleep(Duration::from_millis(1000)).await;
    assert!(
        laptop.graph().roots().is_empty(),
        "1 s since the latest typing, and 1.5 s to wait"
    );

    converged(&[&laptop, &phone]).await;
    assert_eq!(laptop.graph().roots().len(), 2, "sent once typing stopped");
    shut_down(routers, task).await;
}

#[tokio::test]
async fn edits_in_a_block_someone_has_open_are_sent_at_once() {
    let (laptop, phone, routers, task) = connected_pair(Duration::from_secs(60)).await;
    laptop.set_focus(["blk"]);
    eventually("the phone to see it", || phone.watched("blk")).await;

    type_into(&phone, "blk", "typed");
    converged(&[&laptop, &phone]).await;
    assert_eq!(laptop.graph().roots().len(), 1);

    // A block nobody has open is still held.
    type_into(&phone, "elsewhere", "private");
    tokio::time::sleep(SETTLE).await;
    assert_eq!(laptop.graph().roots().len(), 1);
    shut_down(routers, task).await;
}

#[tokio::test]
async fn held_edits_are_sent_when_someone_opens_the_block() {
    let (laptop, phone, routers, task) = connected_pair(Duration::from_secs(60)).await;
    type_into(&phone, "blk", "typed");
    tokio::time::sleep(SETTLE).await;
    assert!(laptop.graph().roots().is_empty());

    laptop.set_focus(["blk"]);
    converged(&[&laptop, &phone]).await;
    assert_eq!(laptop.graph().roots().len(), 1);
    shut_down(routers, task).await;
}

#[tokio::test]
async fn moving_to_another_block_ends_the_edit() {
    let (laptop, phone, routers, task) = connected_pair(Duration::from_secs(60)).await;
    phone.set_focus(["blk"]);
    type_into(&phone, "blk", "typed");
    phone.set_focus(["blk", "other"]);
    tokio::time::sleep(SETTLE).await;
    assert!(laptop.graph().roots().is_empty(), "still in the block");

    phone.set_focus(["other"]);
    converged(&[&laptop, &phone]).await;
    assert_eq!(laptop.graph().roots().len(), 1);
    shut_down(routers, task).await;
}

#[tokio::test]
async fn other_changes_and_ending_the_edit_send_what_is_held() {
    let (laptop, phone, routers, task) = connected_pair(Duration::from_secs(60)).await;
    type_into(&phone, "blk", "typed");
    phone.end_edit();
    converged(&[&laptop, &phone]).await;

    // Anything else that calls `changed` sends the lot, and the edit is over.
    type_into(&phone, "blk", "typed again");
    phone
        .graph()
        .create_block(None, None, "structural")
        .unwrap();
    phone.graph().commit();
    phone.changed();
    converged(&[&laptop, &phone]).await;
    assert_eq!(laptop.graph().roots().len(), 3);

    // Nothing is left pending: ending the edit again changes nothing, and
    // the next typing is held afresh.
    type_into(&phone, "blk", "and again");
    tokio::time::sleep(SETTLE).await;
    assert_eq!(laptop.graph().roots().len(), 3);
    shut_down(routers, task).await;
}

#[tokio::test]
async fn the_idle_time_is_the_one_set() {
    // Well under the default of two seconds.
    let (laptop, phone, routers, task) = connected_pair(Duration::from_millis(100)).await;
    type_into(&phone, "blk", "typed");
    let both = [&laptop, &phone];
    let arrived = tokio::time::timeout(Duration::from_secs(1), converged(&both)).await;
    assert!(arrived.is_ok(), "sent after 100 ms, not the default");
    shut_down(routers, task).await;
}

/// A peer whose device ID is its endpoint ID, as a real device's is, so
/// that the graph's device list can name it.
async fn member(graph: Graph) -> (Peer, Router, EndpointAddr) {
    let endpoint = Endpoint::builder(presets::Minimal)
        .relay_mode(RelayMode::Disabled)
        .bind_addr(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .unwrap()
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .unwrap();
    let addr = EndpointAddr::new(endpoint.id()).with_ip_addr(endpoint.bound_sockets()[0]);
    let peer = Peer::new(
        graph,
        Allowed::Only(HashSet::new()),
        endpoint.id().to_string(),
    );
    let router = Router::builder(endpoint).accept(ALPN, peer.clone()).spawn();
    (peer, router, addr)
}

async fn dial(peer: &Peer, router: &Router, to: &EndpointAddr) -> Result<(), quiddity_node::Error> {
    let idle = Some(Duration::from_secs(2));
    peer.connect(router.endpoint(), to.clone(), idle).await
}

/// Being in the graph's device list is what lets a device connect.
#[tokio::test]
async fn listed_devices_may_connect_and_others_may_not() {
    let (home, home_router, home_addr) = member(Graph::new()).await;
    edit(&home, "private");
    let (guest, guest_router, _) = member(Graph::blank()).await;

    let refused = dial(&guest, &guest_router, &home_addr).await;
    assert!(refused.is_err(), "nobody has listed the guest");
    assert!(guest.graph().roots().is_empty());

    home.allow(&guest_router.endpoint().id().to_string())
        .unwrap();
    dial(&guest, &guest_router, &home_addr).await.unwrap();
    assert_eq!(guest.graph().roots().len(), 1);

    home_router.shutdown().await.unwrap();
    guest_router.shutdown().await.unwrap();
}

/// A device listed by one device may connect to all of them, including ones
/// it has never met, once they have the list.
#[tokio::test]
async fn being_listed_spreads_through_the_graph() {
    let (a, a_router, a_addr) = member(Graph::new()).await;
    edit(&a, "shared");
    let (b, b_router, b_addr) = member(Graph::blank()).await;
    let (c, c_router, _) = member(Graph::blank()).await;

    a.allow(&b_router.endpoint().id().to_string()).unwrap();
    dial(&b, &b_router, &a_addr).await.unwrap();
    assert_eq!(b.graph().roots().len(), 1);

    // Only a knows about c, until b hears.
    a.allow(&c_router.endpoint().id().to_string()).unwrap();
    dial(&b, &b_router, &a_addr).await.unwrap();
    let c_id = c_router.endpoint().id().to_string();
    assert!(b.graph().devices().iter().any(|d| d.id == c_id));

    dial(&c, &c_router, &b_addr).await.unwrap();
    assert_eq!(c.graph().roots().len(), 1, "c syncs with b, not a");

    for router in [a_router, b_router, c_router] {
        router.shutdown().await.unwrap();
    }
}

/// A blank device has no history to list anyone in: it must join first.
#[tokio::test]
async fn a_blank_device_cannot_authorise_anyone() {
    let blank = Peer::new(Graph::blank(), Allowed::Any, "blank");
    assert!(matches!(
        blank.allow("someone"),
        Err(quiddity_node::AllowError::NotJoined)
    ));
    assert!(blank.graph().devices().is_empty());
}

/// An endpoint with this key, on localhost.
async fn bind(key: &iroh::SecretKey) -> (Endpoint, EndpointAddr) {
    let endpoint = Endpoint::builder(presets::Minimal)
        .relay_mode(RelayMode::Disabled)
        .bind_addr(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .unwrap()
        .secret_key(key.clone())
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .unwrap();
    let addr = EndpointAddr::new(endpoint.id()).with_ip_addr(endpoint.bound_sockets()[0]);
    (endpoint, addr)
}

/// A dialler for `peer`, finding devices in `addresses`.
fn dial_listed(
    peer: &Peer,
    router: &Router,
    addresses: Arc<Mutex<Vec<EndpointAddr>>>,
) -> tokio::task::JoinHandle<()> {
    let (peer, endpoint) = (peer.clone(), router.endpoint().clone());
    peer.set_redial(Duration::from_millis(50), Duration::from_millis(200));
    peer.set_handshake_timeout(Duration::from_millis(500));
    tokio::spawn(async move {
        peer.dial_listed(&endpoint, move |id| {
            let known = addresses.lock().unwrap();
            known
                .iter()
                .find(|a| a.id == id)
                .cloned()
                .unwrap_or_else(|| EndpointAddr::new(id))
        })
        .await;
    })
}

/// A listed device is dialled, and dialled again when it goes away and comes
/// back, wherever it comes back.
#[tokio::test]
async fn listed_devices_are_dialled_and_dialled_again() {
    let b_key = iroh::SecretKey::generate();
    let b_id = b_key.public();
    let (home, home_router, _) = member(Graph::new()).await;
    edit(&home, "before");

    // B accepts the home device because it was told to, as at first
    // pairing; the home device dials it because it lists it.
    let b = Peer::new(
        Graph::blank(),
        Allowed::Only(HashSet::from([home_router.endpoint().id()])),
        b_id.to_string(),
    );
    let (b_endpoint, b_addr) = bind(&b_key).await;
    let b_router = Router::builder(b_endpoint).accept(ALPN, b.clone()).spawn();
    home.allow(&b_id.to_string()).unwrap();

    let addresses = Arc::new(Mutex::new(vec![b_addr]));
    let dialler = dial_listed(&home, &home_router, addresses.clone());
    converged(&[&home, &b]).await;

    // B goes away; the home device edits meanwhile; B comes back elsewhere.
    b_router.shutdown().await.unwrap();
    edit(&home, "while away");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (b_endpoint, b_addr) = bind(&b_key).await;
    *addresses.lock().unwrap() = vec![b_addr];
    let b_router = Router::builder(b_endpoint).accept(ALPN, b.clone()).spawn();
    converged(&[&home, &b]).await;
    assert_eq!(b.graph().roots().len(), 2);

    dialler.abort();
    home_router.shutdown().await.unwrap();
    b_router.shutdown().await.unwrap();
}

/// A device listed after the dialler started is picked up, and a name that
/// isn't an endpoint ID is left alone.
#[tokio::test]
async fn devices_listed_later_are_dialled_and_odd_names_are_not() {
    let (home, home_router, _) = member(Graph::new()).await;
    edit(&home, "shared");
    let dialler = dial_listed(&home, &home_router, Arc::default());
    tokio::time::sleep(Duration::from_millis(200)).await;

    home.allow("not an endpoint id").unwrap();
    let key = iroh::SecretKey::generate();
    let late = Peer::new(
        Graph::blank(),
        Allowed::Only(HashSet::from([home_router.endpoint().id()])),
        key.public().to_string(),
    );
    let (endpoint, addr) = bind(&key).await;
    let late_router = Router::builder(endpoint).accept(ALPN, late.clone()).spawn();
    dialler.abort();
    // Restart with the late device's address known, now it is listed.
    home.allow(&key.public().to_string()).unwrap();
    let dialler = dial_listed(&home, &home_router, Arc::new(Mutex::new(vec![addr])));
    converged(&[&home, &late]).await;
    assert!(!dialler.is_finished(), "the odd name didn't stop it");

    dialler.abort();
    home_router.shutdown().await.unwrap();
    late_router.shutdown().await.unwrap();
}

/// Counts the connections made to it, and closes each at once.
#[derive(Debug, Clone)]
struct Counter(Arc<AtomicUsize>);

impl ProtocolHandler for Counter {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        connection.close(1u32.into(), b"counted");
        Ok(())
    }
}

/// A device that keeps refusing is tried again at a growing interval (here
/// from 50 ms to 200 ms): not hammered, and not forgotten.
#[tokio::test]
async fn a_listed_device_that_refuses_is_retried_with_backoff() {
    let (home, home_router, _) = member(Graph::new()).await;
    let key = iroh::SecretKey::generate();
    let (endpoint, addr) = bind(&key).await;
    let attempts = Arc::new(AtomicUsize::new(0));
    let router = Router::builder(endpoint)
        .accept(ALPN, Counter(attempts.clone()))
        .spawn();
    home.allow(&key.public().to_string()).unwrap();

    let dialler = dial_listed(&home, &home_router, Arc::new(Mutex::new(vec![addr])));
    tokio::time::sleep(Duration::from_millis(1500)).await;
    // About nine: at 0, 50, 150, then every 200 ms. Retrying every 50 ms
    // would be thirty, and the default delay of a second would be two.
    let n = attempts.load(Ordering::SeqCst);
    assert!((5..=14).contains(&n), "{n} attempts in 1.5 s");

    dialler.abort();
    home_router.shutdown().await.unwrap();
    router.shutdown().await.unwrap();
}

/// An address nobody answers at is given up on, not waited on for as long
/// as QUIC would.
#[tokio::test]
async fn a_dead_address_is_given_up_on() {
    let (home, home_router, _) = member(Graph::new()).await;
    let (endpoint, addr) = bind(&iroh::SecretKey::generate()).await;
    endpoint.close().await;
    home.set_handshake_timeout(Duration::from_millis(300));

    let result = tokio::time::timeout(
        Duration::from_secs(3),
        home.connect(home_router.endpoint(), addr, None),
    )
    .await;
    assert!(
        matches!(result, Ok(Err(quiddity_node::Error::TimedOut))),
        "{result:?}"
    );
    home_router.shutdown().await.unwrap();
}

/// The dialler asks where the other devices are, and never about itself.
#[tokio::test]
async fn the_dialler_does_not_dial_itself() {
    let (home, home_router, _) = member(Graph::new()).await;
    let other = iroh::SecretKey::generate().public();
    // The graph lists this device as well, as a saved graph does.
    home.allow(&home_router.endpoint().id().to_string())
        .unwrap();
    home.allow(&other.to_string()).unwrap();

    let asked = Arc::new(Mutex::new(Vec::new()));
    let (peer, endpoint, log) = (home.clone(), home_router.endpoint().clone(), asked.clone());
    let dialler = tokio::spawn(async move {
        peer.dial_listed(&endpoint, move |id| {
            log.lock().unwrap().push(id);
            EndpointAddr::new(id)
        })
        .await;
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    dialler.abort();

    {
        let asked = asked.lock().unwrap();
        assert!(asked.contains(&other), "{asked:?}");
        assert!(!asked.contains(&home_router.endpoint().id()), "{asked:?}");
    }
    home_router.shutdown().await.unwrap();
}

/// A device that was trusted for now may connect, and is listed by the
/// connection, so that it keeps being let in.
#[tokio::test]
async fn a_trusted_device_may_connect_and_is_listed_by_it() {
    let (home, home_router, home_addr) = member(Graph::new()).await;
    edit(&home, "shared");
    let (guest, guest_router, _) = member(Graph::blank()).await;
    let guest_id = guest_router.endpoint().id();

    assert!(dial(&guest, &guest_router, &home_addr).await.is_err());
    assert!(
        !home
            .graph()
            .devices()
            .iter()
            .any(|d| d.id == guest_id.to_string())
    );

    home.trust(guest_id);
    dial(&guest, &guest_router, &home_addr).await.unwrap();
    assert_eq!(guest.graph().roots().len(), 1);
    assert!(
        home.graph()
            .devices()
            .iter()
            .any(|d| d.id == guest_id.to_string())
    );

    home_router.shutdown().await.unwrap();
    guest_router.shutdown().await.unwrap();
}

/// `serve` puts a peer on the network: others can reach it by the address of
/// its endpoint, and shutting it down closes the endpoint.
#[tokio::test]
async fn serve_puts_a_peer_on_the_network() {
    let key = iroh::SecretKey::generate();
    let home = Peer::new(Graph::new(), Allowed::Any, key.public().to_string());
    edit(&home, "served");
    let local = Some(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)));
    let server = quiddity_node::serve(&home, key, local).await.unwrap();
    let addr = EndpointAddr::new(server.endpoint().id())
        .with_ip_addr(server.endpoint().bound_sockets()[0]);

    let (guest, guest_router, _) = member(Graph::blank()).await;
    dial(&guest, &guest_router, &addr).await.unwrap();
    assert_eq!(guest.graph().roots().len(), 1);

    let endpoint = server.endpoint().clone();
    server.shutdown().await.unwrap();
    assert!(endpoint.is_closed());
    guest_router.shutdown().await.unwrap();
}

/// Admitting lists a device in a graph that has history, and only trusts
/// one in a blank replica, which has none to list anyone in.
#[tokio::test]
async fn admitting_a_device_lists_it_or_trusts_it() {
    let key = iroh::SecretKey::generate().public().to_string();
    let joined = Peer::new(Graph::new(), Allowed::Only(HashSet::new()), "joined");
    joined.admit(&key).unwrap();
    assert!(joined.graph().devices().iter().any(|d| d.id == key));

    let (home, home_router, _) = member(Graph::new()).await;
    edit(&home, "shared");
    let (blank, blank_router, _) = member(Graph::blank()).await;
    blank
        .admit(&home_router.endpoint().id().to_string())
        .unwrap();
    assert!(blank.graph().devices().is_empty(), "nothing to list it in");
    // Trusted, so the home device can reach the blank one (which accepts
    // only what it is told to), and the join lists it from there.
    let blank_addr = EndpointAddr::new(blank_router.endpoint().id())
        .with_ip_addr(blank_router.endpoint().bound_sockets()[0]);
    home.admit(&blank_router.endpoint().id().to_string())
        .unwrap();
    dial(&home, &home_router, &blank_addr).await.unwrap();
    assert_eq!(blank.graph().roots().len(), 1);
    assert!(!blank.graph().devices().is_empty());

    assert!(matches!(
        joined.admit("not a device"),
        Err(quiddity_node::AllowError::NotADevice(_))
    ));
    home_router.shutdown().await.unwrap();
    blank_router.shutdown().await.unwrap();
}
