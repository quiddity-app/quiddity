//! Syncing a graph with other devices over iroh.
//!
//! A [`Peer`] owns a graph and runs a [`quiddity_sync::Session`] over every
//! connection, in either direction. When the graph changes, whether by a
//! local edit or by updates from one peer, every connection is flushed, so
//! a peer in the middle relays between the others. The headless sync node
//! is just a peer that is always on.
//!
//! The same connections carry presence: which blocks each device has open.
//! It is kept in memory, apart from the graph.
//!
//! Typing is not sent keystroke by keystroke to devices that can't see it:
//! see [`Peer::edited`].

pub mod blob_store;
pub mod blobs;
pub mod debug_http;
pub mod events;
pub mod gc;
pub mod keyring;
pub mod legacy_blobs;
mod link_new;
pub mod places;
pub mod store;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use events::{Events, Level};
use iroh::endpoint::{
    Connection, ConnectionError, ReadError, RecvStream, SendStream, WriteError, presets,
};
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode, SecretKey, Watcher};
use jiff::Timestamp;
use quiddity_core::{Graph, Index, Version, merge_duplicates};
use quiddity_sync::{
    DecodeError, FrameError, FrameReader, Message, Presence, Record, Session, State, write_frame,
};
use tokio::sync::watch;
use tokio::task::JoinSet;
use tracing::{debug, error, info, instrument, warn};

/// The protocol's name, negotiated when connecting.
pub const ALPN: &[u8] = b"quiddity/sync/3";

/// How long to wait before dialling a listed device again, at first and at
/// most (it doubles in between, so a device that is off isn't pestered).
///
/// The ceiling is long because the devices that are often away, a phone, will
/// dial in when they are back; this is only the way to find one that can't.
/// Starting the app and a change of network begin again at the minimum.
const REDIAL_MIN: Duration = Duration::from_secs(10);
const REDIAL_MAX: Duration = Duration::from_hours(24);

/// How long to wait for a connection to be made, before giving up and, for a
/// dialler, trying again. Waiting out a dead address takes much longer.
const HANDSHAKE: Duration = Duration::from_secs(10);

/// How long to wait before looking again for blobs that no connected device
/// has given us.
const BLOB_RETRY: Duration = Duration::from_secs(30);

/// How long after the last keystroke an edit nobody is watching is sent.
const EDIT_IDLE: Duration = Duration::from_secs(2);

/// The code a device closes a connection with when the peer is behind its
/// history, so the peer can say why rather than that the connection went.
const BEHIND: u32 = 2;

/// The code a device closes a connection with when the peer holds another
/// graph, for the same reason.
const OTHER_GRAPH: u32 = 3;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Connect(#[from] iroh::endpoint::ConnectError),
    #[error(transparent)]
    Connection(#[from] ConnectionError),
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Write(#[from] WriteError),
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error(transparent)]
    Sync(#[from] quiddity_sync::Error),
    #[error("timed out making the connection")]
    TimedOut,
    #[error("the peer's graph uses schema {0:?}, which this build can't sync with")]
    Incompatible(Option<i64>),
    #[error("the peer holds another graph, and this device won't merge two")]
    OtherGraph,
    #[error("this device is behind the peer's history: it needs to start again from a snapshot")]
    Behind,
    #[error("the peer is behind this device's history: it needs to start again from a snapshot")]
    PeerBehind,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{} already holds a graph", .0.display())]
    Exists(PathBuf),
    /// There is no graph to load: `init` makes one.
    #[error("{} holds no graph: create one with `init`", .0.display())]
    Missing(PathBuf),
    /// A file is there, but it isn't a graph this build can read.
    #[error("{} is not a graph this build can read: {}", .0.display(), .1)]
    Unreadable(PathBuf, String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// This device's key is there but isn't one: it is never replaced.
    #[error("this device's key, in {}, can't be used: {}", .0.display(), .1)]
    DeviceKey(PathBuf, String),
    #[error("the graph's file: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Graph(#[from] quiddity_core::Error),
}

/// Which devices may connect.
///
/// The graph's device list is the standing answer: a device listed there
/// may always connect, whatever else is said here. `Only` adds devices that
/// aren't listed yet, which is how the first is let in; a device that
/// connects is listed (see `Session`), so from then on the graph says so,
/// to every device. The list decides who may connect, not whose edits are
/// kept: edits are merged whoever made them.
#[derive(Debug, Clone)]
pub enum Allowed {
    Any,
    Only(HashSet<EndpointId>),
}

#[derive(Debug, thiserror::Error)]
pub enum AllowError {
    #[error("{0:?} is not a device ID")]
    NotADevice(String),
    #[error("this device has not joined a graph yet: sync before authorising others")]
    NotJoined,
    #[error(transparent)]
    Graph(#[from] quiddity_core::Error),
}

/// Lists `device` in the graph, so that every device that has this edit
/// lets it connect. Nothing is recorded for a blank replica: an edit with
/// nothing beneath it couldn't be applied by a compacted device.
///
/// Until the device has synced and recorded what it has stored, no device
/// compacts past what it might lack.
pub fn allow_device(graph: &Graph, device: &str, now: Timestamp) -> Result<(), AllowError> {
    if graph.version() == Version::default() {
        return Err(AllowError::NotJoined);
    }
    graph.vouch_for(device, &Version::default(), now)?;
    Ok(())
}

/// A graph, shared with every device it is connected to.
#[derive(Debug, Clone)]
pub struct Peer {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    /// This device's ID in the graph's device list.
    device: String,
    graph: Graph,
    allowed: Allowed,
    /// Devices let in for now, not listed in the graph: see [`Peer::trust`].
    trusted: Mutex<HashSet<EndpointId>>,
    /// Bumped when a device is trusted, so the dialler picks it up.
    trust_changes: watch::Sender<u64>,
    /// Bumped whenever the graph changes, so that it is saved.
    changes: watch::Sender<u64>,
    /// Bumped when the changes so far are to be sent, so every connection
    /// flushes. It follows `changes` except while edits are held back.
    publish: watch::Sender<u64>,
    gate: Mutex<Gate>,
    dialling: Mutex<Dialling>,
    /// Bumped when any device's connection ends, so dialling tasks look at
    /// their own device's epoch.
    link_changes: watch::Sender<u64>,
    /// Bumped by [`Peer::retry_now`], so every dialler starts its waits again.
    redial_resets: watch::Sender<u64>,
    /// Bumped when the gate's deadline moves, so connections re-read it.
    gate_changes: watch::Sender<u64>,
    presence: Mutex<PresenceState>,
    /// Bumped whenever presence changes, so every connection announces it.
    presence_changes: watch::Sender<u64>,
    /// What each connected peer is known to have, by connection, for
    /// [`Peer::sync_state`], with the device each connection is to.
    syncs: Mutex<BTreeMap<u64, (String, Version)>>,
    next_link: AtomicU64,
    /// Bumped whenever [`Peer::sync_state`] may have changed.
    status_changes: watch::Sender<u64>,
    /// The bytes of the graph's binaries, if this device keeps them.
    blobs: Mutex<Option<blobs::Blobs>>,
    /// What has lately happened on the network: see [`Peer::events`].
    events: Mutex<Events>,
}

/// Edits held back because nobody else has their block open.
#[derive(Debug)]
struct Gate {
    /// The blocks with edits that have not been sent.
    pending: BTreeSet<String>,
    /// When they are sent regardless: `idle` after the latest edit.
    deadline: Option<tokio::time::Instant>,
    idle: Duration,
}

/// How a peer dials: how long it waits between attempts, at first and at
/// most, and how long it waits for one to connect.
#[derive(Debug, Clone, Copy)]
struct Dialling {
    min: Duration,
    max: Duration,
    handshake: Duration,
}

#[derive(Debug)]
struct PresenceState {
    presence: Presence,
    /// How many connections each connected device has. Two devices that
    /// dial each other at once have two.
    links: BTreeMap<String, usize>,
    /// How many connections to each device have ended, whichever side dialled
    /// them, so the dialler can tell that one did while it was waiting or
    /// dialling. A connection coming up needs no count: nothing is dialled
    /// while it lasts, and its ending starts the waits again.
    link_epochs: BTreeMap<String, u64>,
}

/// A connection that has been announced as a link in the presence network,
/// and is withdrawn when the connection ends, however it ends.
struct Link<'a> {
    peer: &'a Peer,
    /// Tells this connection from the others, for `Peer::syncs`.
    id: u64,
    device: Option<String>,
}

impl Drop for Link<'_> {
    fn drop(&mut self) {
        self.peer.syncs().remove(&self.id);
        if let Some(device) = &self.device {
            self.peer.disconnected(device);
        }
        bump(&self.peer.inner.status_changes);
    }
}

impl Peer {
    /// A peer for `graph`, as the device `device` (its ID in the graph's
    /// device list: the endpoint ID, for a real device).
    pub fn new(graph: Graph, allowed: Allowed, device: impl Into<String>) -> Self {
        let device = device.into();
        // Later than any earlier run's, so that a restart's records
        // outrank what peers remember (unless the clock has gone back).
        let epoch = u64::try_from(Timestamp::now().as_microsecond()).unwrap_or(0);
        Self {
            inner: Arc::new(Inner {
                presence: Mutex::new(PresenceState {
                    presence: Presence::new(device.clone(), epoch),
                    links: BTreeMap::new(),
                    link_epochs: BTreeMap::new(),
                }),
                link_changes: watch::Sender::new(0),
                presence_changes: watch::Sender::new(0),
                syncs: Mutex::new(BTreeMap::new()),
                next_link: AtomicU64::new(0),
                status_changes: watch::Sender::new(0),
                blobs: Mutex::new(None),
                events: Mutex::new(Events::default()),
                gate: Mutex::new(Gate {
                    pending: BTreeSet::new(),
                    deadline: None,
                    idle: EDIT_IDLE,
                }),
                dialling: Mutex::new(Dialling {
                    min: REDIAL_MIN,
                    max: REDIAL_MAX,
                    handshake: HANDSHAKE,
                }),
                redial_resets: watch::Sender::new(0),
                gate_changes: watch::Sender::new(0),
                publish: watch::Sender::new(0),
                device,
                graph,
                allowed,
                trusted: Mutex::new(HashSet::new()),
                trust_changes: watch::Sender::new(0),
                changes: watch::Sender::new(0),
            }),
        }
    }

    #[must_use]
    pub fn graph(&self) -> &Graph {
        &self.inner.graph
    }

    /// This device's ID in the graph's device list: the endpoint ID, for a
    /// real device.
    #[must_use]
    pub fn device(&self) -> &str {
        &self.inner.device
    }

    /// Authorises `device` (an endpoint ID) to connect to every device that
    /// shares this graph, by listing it in the graph.
    pub fn allow(&self, device: &str) -> Result<(), AllowError> {
        allow_device(self.graph(), device, Timestamp::now())?;
        self.changed();
        Ok(())
    }

    /// What this peer has lately done on the network, oldest first: who it
    /// dialled, who dialled it, who it refused and why connections ended. It is
    /// for working out why devices don't sync, and says nothing of the graph's
    /// content.
    #[must_use]
    pub fn events(&self) -> Vec<events::Event> {
        self.inner
            .events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .all()
    }

    /// Keeps `message` in [`Peer::events`], and tells whoever is watching the
    /// status, since the log is part of it. The network's owner uses it for
    /// what the peer can't see itself, such as whether the endpoint started.
    pub fn record(&self, level: Level, device: Option<&str>, message: String) {
        self.inner
            .events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(level, device, message);
        bump(&self.inner.status_changes);
    }

    /// Records how a connection to `device` ended: a peer going away is
    /// routine, and anything else is why devices may not be syncing.
    fn record_end(&self, device: &str, how: &str, result: &Result<(), Error>) {
        match result {
            Ok(()) => self.record(Level::Info, Some(device), format!("{how} ended")),
            // The other device shut the door on purpose, and said why.
            Err(e) if refused_by_peer(e).is_some() => self.record(
                Level::Problem,
                Some(device),
                format!(
                    "{how} ended: the other device closed it ({})",
                    refused_by_peer(e).unwrap_or_default()
                ),
            ),
            Err(e) if peer_went_away(e) => self.record(
                Level::Info,
                Some(device),
                format!("{how} ended: the connection was lost ({e})"),
            ),
            Err(e) => self.record(Level::Problem, Some(device), format!("{how} ended: {e}")),
        }
    }

    /// How long to wait before dialling a device again, at first and at
    /// most.
    pub fn set_redial(&self, min: Duration, max: Duration) {
        let mut dialling = self.dialling();
        dialling.min = min;
        dialling.max = max;
    }

    /// How long to wait for a connection to be made.
    pub fn set_handshake_timeout(&self, timeout: Duration) {
        self.dialling().handshake = timeout;
    }

    /// Starts the waits between dialling attempts again: every listed device
    /// that isn't connected is dialled now, and then after the shortest wait.
    /// For when something has changed that may make a device reachable, such
    /// as the network. `dial_listed` calls it when the endpoint's addresses
    /// change; a platform that can see the network change and iroh can't (as
    /// on Android) should call it too.
    pub fn retry_now(&self) {
        bump(&self.inner.redial_resets);
    }

    fn dialling(&self) -> MutexGuard<'_, Dialling> {
        self.inner
            .dialling
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Keeps a connection up to every device the graph lists, other than
    /// this one, for as long as it is polled (spawn it, and abort the task to
    /// stop). A device listed later is picked up as soon as the listing
    /// arrives. `address` says how to find a device from its ID; for a
    /// device on the internet that is just its ID, and iroh looks the rest
    /// up. It is asked again for every attempt, so a device that comes back
    /// somewhere else is found.
    ///
    /// Each device is dialled on its own: a failure is retried after a delay
    /// that doubles to a day, so a device that is off, or only ever
    /// dials in, isn't pestered. The delay starts again when this dialler
    /// does, when the endpoint's addresses change (the network did), on
    /// [`Peer::retry_now`], and when a connection to the device ends. A device
    /// that is no longer listed or trusted is dropped. Two devices
    /// that both dial may end up with two connections between them, which
    /// works but sends everything twice.
    pub async fn dial_listed(
        &self,
        endpoint: &Endpoint,
        address: impl Fn(EndpointId) -> EndpointAddr + Send + Sync + 'static,
    ) {
        let address = Arc::new(address);
        let mut changes = self.subscribe();
        let mut trust_changes = self.inner.trust_changes.subscribe();
        let mut dialling: HashMap<String, tokio::task::AbortHandle> = HashMap::new();
        let mut tasks = JoinSet::new();
        // A new address means the network changed, and a device that was out of
        // reach may not be now.
        let (peer, mut addresses) = (self.clone(), endpoint.watch_addr());
        tasks.spawn(async move {
            while addresses.updated().await.is_ok() {
                peer.retry_now();
            }
        });
        loop {
            // The devices to keep up: those the graph lists and those let in
            // for now. A replica that has joined nothing lists no one, and
            // would wait for ever to be dialled by a device that may be asleep
            // or on a network that can't reach this one.
            let mut wanted: Vec<String> =
                self.graph().devices().into_iter().map(|d| d.id).collect();
            wanted.extend(self.trusted_devices().iter().map(ToString::to_string));
            // Stop dialling a device that is no longer wanted, such as one
            // that has been removed from the list. A device that comes back is
            // started afresh below.
            dialling.retain(|device, task| {
                let keep = wanted.contains(device);
                if !keep {
                    task.abort();
                }
                keep
            });
            for device in wanted {
                // A name that isn't an endpoint ID can't be dialled.
                let Ok(id) = device.parse::<EndpointId>() else {
                    continue;
                };
                if device == self.inner.device || dialling.contains_key(&device) {
                    continue;
                }
                let (peer, endpoint, address) =
                    (self.clone(), endpoint.clone(), Arc::clone(&address));
                let name = device.clone();
                let task = tasks.spawn(async move {
                    peer.keep_connected(&endpoint, &device, id, &*address).await;
                });
                dialling.insert(name, task);
            }
            tokio::select! {
                changed = changes.changed() => {
                    if changed.is_err() {
                        return;
                    }
                }
                changed = trust_changes.changed() => {
                    if changed.is_err() {
                        return;
                    }
                }
            }
        }
    }

    /// Dials `device` whenever it isn't connected, forever.
    async fn keep_connected(
        &self,
        endpoint: &Endpoint,
        device: &str,
        id: EndpointId,
        address: &(dyn Fn(EndpointId) -> EndpointAddr + Send + Sync),
    ) {
        let Dialling { min, max, .. } = *self.dialling();
        let mut delay = min;
        let mut resets = self.inner.redial_resets.subscribe();
        let mut link_changes = self.inner.link_changes.subscribe();
        let epoch = || {
            self.presence()
                .link_epochs
                .get(device)
                .copied()
                .unwrap_or(0)
        };
        let mut seen = epoch();
        loop {
            if !self.presence().links.contains_key(device) {
                debug!(device, "dialling");
                self.record(Level::Info, Some(device), "dialling".to_owned());
                // Failing to reach a device is routine, such as when it is
                // off: say so for someone looking, and try again.
                if let Err(e) = self.connect(endpoint, address(id), None).await {
                    debug!(device, error = %e, "connection ended");
                }
            }
            // A connection to the device ending, whoever dialled it, starts the
            // waits again, though one that ended during the dial counts too.
            // Connected, there is nothing to wait out: the next thing to look
            // for is the connection ending.
            let mut wait_again = true;
            while wait_again {
                if epoch() != seen {
                    seen = epoch();
                    delay = min;
                }
                let linked = self.presence().links.contains_key(device);
                let deadline = (!linked).then(|| tokio::time::Instant::now() + delay);
                wait_again = false;
                loop {
                    tokio::select! {
                        () = async {
                            match deadline {
                                Some(at) => tokio::time::sleep_until(at).await,
                                None => std::future::pending().await,
                            }
                        } => {
                            delay = (delay * 2).min(max);
                            break;
                        }
                        _ = resets.changed() => {
                            delay = min;
                            break;
                        }
                        _ = link_changes.changed() => {
                            // Some other device's, perhaps: keep waiting.
                            if epoch() != seen {
                                wait_again = true;
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Lets `device` connect, until this peer stops. It is not recorded in
    /// the graph: a replica that has joined nothing yet has no history to
    /// list anyone in, and needs to let in the device it will join through.
    /// That device lists it when it connects, which does last.
    ///
    /// A trusted device is dialled as well as waited for (see
    /// [`Peer::dial_listed`]), so a replica waiting for a graph reaches the
    /// device that has it without that device having to reach it.
    pub fn trust(&self, device: EndpointId) {
        self.inner
            .trusted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(device);
        bump(&self.inner.trust_changes);
    }

    /// The devices let in for now: see [`Peer::trust`].
    fn trusted_devices(&self) -> Vec<EndpointId> {
        self.inner
            .trusted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .copied()
            .collect()
    }

    /// Whether `device` may connect: for sync, and for fetching blobs.
    fn permits(&self, device: &EndpointId) -> bool {
        match &self.inner.allowed {
            Allowed::Any => true,
            Allowed::Only(ids) => {
                ids.contains(device) || self.trusted(device) || self.listed(device)
            }
        }
    }

    /// Has this peer keep the bytes of the graph's binaries in `blobs`: it
    /// gives them to the devices that may connect, and, once served (see
    /// [`serve`]), fetches those the graph names that it lacks.
    pub fn set_blobs(&self, blobs: blobs::Blobs) {
        *self
            .inner
            .blobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(blobs);
    }

    /// The blob store, if this peer keeps one.
    pub fn blobs(&self) -> Option<blobs::Blobs> {
        self.inner
            .blobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The devices connected now, as far as the sync protocol knows.
    #[must_use]
    pub fn connected_devices(&self) -> Vec<EndpointId> {
        let state = self.presence();
        state
            .links
            .keys()
            .filter_map(|device| device.parse().ok())
            .collect()
    }

    /// The hashes the graph names as binaries that `blobs` doesn't have.
    pub async fn missing_blobs(&self, blobs: &blobs::Blobs) -> Vec<blobs::Hash> {
        let Ok(index) = Index::build(self.graph()) else {
            return Vec::new();
        };
        let mut missing = Vec::new();
        for (_, asset) in index.assets() {
            let Ok(hash) = asset.hash.parse::<blobs::Hash>() else {
                continue;
            };
            if !missing.contains(&hash) && !blobs.has(hash).await.unwrap_or(true) {
                missing.push(hash);
            }
        }
        missing
    }

    /// Fetches the blobs the graph names that `blobs` lacks, from the devices
    /// that are connected, and goes on doing so: when the graph changes, when
    /// a device connects, and, while any are missing, every so often, since
    /// the device that has one may not be here yet. A device with a blob it
    /// can't give, or that gives a different one, is passed over for the next.
    pub async fn fetch_blobs(
        &self,
        endpoint: &Endpoint,
        blobs: &blobs::Blobs,
        address: impl Fn(EndpointId) -> EndpointAddr,
    ) {
        let (mut graph, mut devices) = (self.subscribe(), self.subscribe_status());
        let mut reported = HashSet::new();
        loop {
            let missing = self.missing_blobs(blobs).await;
            for hash in &missing {
                for device in self.connected_devices() {
                    match fetch_from(endpoint, blobs, address(device), *hash).await {
                        Ok(()) => break,
                        Err(e) if reported.insert((device, *hash)) => {
                            warn!(%hash, %device, error = %e, "fetching a blob failed");
                        }
                        Err(_) => {}
                    }
                }
            }
            let retry = tokio::time::sleep(BLOB_RETRY);
            tokio::select! {
                changed = graph.changed() => if changed.is_err() { return },
                changed = devices.changed() => if changed.is_err() { return },
                () = retry, if !missing.is_empty() => {}
            }
        }
    }

    fn trusted(&self, device: &EndpointId) -> bool {
        self.inner
            .trusted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(device)
    }

    /// Lets the device `device` (an endpoint ID, as text) in: lists it in the
    /// graph, or, if this replica has not joined a graph yet and so has none
    /// to list it in, trusts it for now (see [`Peer::trust`]). It is the
    /// action behind pairing two devices.
    pub fn admit(&self, device: &str) -> Result<(), AllowError> {
        let id: EndpointId = device
            .parse()
            .map_err(|_| AllowError::NotADevice(device.to_owned()))?;
        if self.graph().version() == Version::default() {
            self.trust(id);
            Ok(())
        } else {
            self.allow(device)
        }
    }

    /// Whether the graph lists `device`.
    fn listed(&self, device: &EndpointId) -> bool {
        let device = device.to_string();
        self.graph().devices().iter().any(|d| d.id == device)
    }

    /// Call after committing local changes, to send them, and any edits
    /// still held back (see [`Peer::edited`]), to connected peers now.
    pub fn changed(&self) {
        bump(&self.inner.changes);
        self.publish_now();
    }

    /// Call after committing an edit to the text of `block`, typing say,
    /// which many follow one another. It is saved at once, but sent to
    /// connected devices only if one of them has the block open. Otherwise
    /// it waits, with whatever follows it, until the edit has ended: no
    /// further edit for [`Peer::set_edit_idle`], this device moving to
    /// another block ([`Peer::set_focus`]), [`Peer::end_edit`], or anything
    /// that calls [`Peer::changed`]. The history is the same either way:
    /// it is when it is sent that differs.
    pub fn edited(&self, block: impl Into<String>) {
        bump(&self.inner.changes);
        {
            let mut gate = self.gate();
            gate.pending.insert(block.into());
            gate.deadline = Some(tokio::time::Instant::now() + gate.idle);
        }
        if self.pending_watched() {
            self.publish_now();
        } else {
            bump(&self.inner.gate_changes);
            // Held back, so peers are behind.
            bump(&self.inner.status_changes);
        }
    }

    /// The edit in progress has ended (the caret has gone, say): send what
    /// was held back.
    pub fn end_edit(&self) {
        if !self.gate().pending.is_empty() {
            self.publish_now();
        }
    }

    /// How long after the latest edit an unwatched one is sent regardless.
    pub fn set_edit_idle(&self, idle: Duration) {
        self.gate().idle = idle;
    }

    fn gate(&self) -> MutexGuard<'_, Gate> {
        self.inner
            .gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether another device has a block open that has held edits.
    fn pending_watched(&self) -> bool {
        let pending = self.gate().pending.clone();
        let state = self.presence();
        pending.iter().any(|block| state.presence.watched(block))
    }

    /// Sends whatever has been held back, and everything else.
    fn publish_now(&self) {
        {
            let mut gate = self.gate();
            gate.pending.clear();
            gate.deadline = None;
        }
        bump(&self.inner.gate_changes);
        bump(&self.inner.publish);
        bump(&self.inner.status_changes);
    }

    /// The deadline has passed, unless more edits have moved it.
    fn publish_if_due(&self) {
        let due = self
            .gate()
            .deadline
            .is_some_and(|d| d <= tokio::time::Instant::now());
        if due {
            self.publish_now();
        }
    }

    /// Says which blocks this device has open, to every device it can reach.
    ///
    /// Moving away from a block ends any edit held back in it.
    pub fn set_focus<S: Into<String>>(&self, blocks: impl IntoIterator<Item = S>) {
        let focus: BTreeSet<String> = blocks.into_iter().map(Into::into).collect();
        let left = self.gate().pending.iter().any(|b| !focus.contains(b));
        self.presence().presence.set_focus(focus);
        self.presence_changed();
        if left {
            self.publish_now();
        }
    }

    /// Whether another device, one we are connected to or can reach through
    /// those that are, has `block` open.
    #[must_use]
    pub fn watched(&self, block: &str) -> bool {
        self.presence().presence.watched(block)
    }

    /// The other devices that are here, and what they have open.
    #[must_use]
    pub fn present(&self) -> Vec<Record> {
        let state = self.presence();
        state.presence.present().into_iter().cloned().collect()
    }

    /// How this device stands with the others, for showing the user. It is
    /// about the devices this one is connected to, which may be relaying for
    /// others.
    #[must_use]
    pub fn sync_state(&self) -> SyncState {
        self.sync_details().state
    }

    /// [`Peer::sync_state`] and what it was worked out from, for showing the
    /// user why.
    #[must_use]
    pub fn sync_details(&self) -> SyncDetails {
        let listed = self
            .graph()
            .devices()
            .iter()
            .filter(|d| d.id != self.inner.device)
            .count();
        let ours = self.graph().version();
        let links: Vec<(String, Version)> = self.syncs().values().cloned().collect();
        let theirs: Vec<Version> = links.iter().map(|(_, v)| v.clone()).collect();
        let held = self.gate().pending.len();
        let watching = self
            .present()
            .iter()
            .filter(|r| !r.focus.is_empty())
            .count();
        let connected = connected_devices(&links, &ours);
        SyncDetails {
            state: classify(listed > 0, &theirs, &ours, held > 0, watching > 0),
            listed,
            connected,
            held,
            watching,
        }
    }

    /// Watches for changes that may change [`Peer::sync_state`].
    #[must_use]
    pub fn subscribe_status(&self) -> watch::Receiver<u64> {
        self.inner.status_changes.subscribe()
    }

    fn syncs(&self) -> MutexGuard<'_, BTreeMap<u64, (String, Version)>> {
        self.inner
            .syncs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Records what a connection's peer is now known to have.
    fn note_sync(&self, link: &Link<'_>, session: &Session) {
        if let Some(device) = &link.device {
            self.syncs()
                .insert(link.id, (device.clone(), session.theirs().clone()));
            bump(&self.inner.status_changes);
        }
    }

    /// Watches for changes to who is here and what they have open.
    #[must_use]
    pub fn subscribe_presence(&self) -> watch::Receiver<u64> {
        self.inner.presence_changes.subscribe()
    }

    fn presence(&self) -> MutexGuard<'_, PresenceState> {
        self.inner
            .presence
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Announces a change to presence. If it puts someone in a block with
    /// held edits, they are sent.
    fn presence_changed(&self) {
        bump(&self.inner.presence_changes);
        bump(&self.inner.status_changes);
        if self.pending_watched() {
            self.publish_now();
        }
    }

    fn connected(&self, device: &str) {
        info!(device, "connected");
        self.record(Level::Info, Some(device), "syncing".to_owned());
        let mut state = self.presence();
        *state.links.entry(device.to_owned()).or_default() += 1;
        state.presence.connect(device);
        drop(state);
        self.presence_changed();
    }

    fn disconnected(&self, device: &str) {
        info!(device, "disconnected");
        self.record(Level::Info, Some(device), "no longer syncing".to_owned());
        let mut state = self.presence();
        *state.link_epochs.entry(device.to_owned()).or_default() += 1;
        let remaining = state.links.get_mut(device).map_or(0, |n| {
            *n -= 1;
            *n
        });
        if remaining == 0 {
            state.links.remove(device);
            state.presence.disconnect(device);
        } else {
            // Another connection to the device is left, and doesn't know
            // what the one that has gone was told: start it afresh.
            state.presence.connect(device);
        }
        drop(state);
        bump(&self.inner.link_changes);
        self.presence_changed();
    }

    /// Merges duplicate entities that arrived from different devices, and
    /// sends the merge on to connected devices. Returns whether anything
    /// moved.
    pub fn tidy(&self) -> Result<bool, quiddity_core::Error> {
        let graph = self.graph();
        let moved = merge_duplicates(graph, &Index::build(graph)?)? > 0;
        if moved {
            debug!("merged duplicates");
            self.changed();
        }
        Ok(moved)
    }

    /// Watches for changes to the graph, local or from peers.
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.inner.changes.subscribe()
    }

    /// Connects to another device and syncs until either side closes the
    /// connection or, with `idle`, until nothing has arrived for that long.
    pub async fn connect(
        &self,
        endpoint: &Endpoint,
        addr: impl Into<EndpointAddr>,
        idle: Option<Duration>,
    ) -> Result<(), Error> {
        let handshake = self.dialling().handshake;
        let addr = addr.into();
        let device = addr.id.to_string();
        let made = tokio::time::timeout(handshake, endpoint.connect(addr, ALPN))
            .await
            .map_err(|_| Error::TimedOut)
            .and_then(|made| made.map_err(Error::from));
        let connection = match made {
            Ok(connection) => connection,
            Err(e) => {
                self.record(
                    Level::Problem,
                    Some(&device),
                    format!("couldn't connect: {e}"),
                );
                return Err(e);
            }
        };
        let result = async {
            let (send, recv) = connection.open_bi().await?;
            self.sync(send, recv, idle).await
        }
        .await;
        close(&connection, &result);
        self.record_end(&device, "dialled connection", &result);
        result
    }

    async fn sync(
        &self,
        mut send: SendStream,
        mut recv: RecvStream,
        idle: Option<Duration>,
    ) -> Result<(), Error> {
        let graph = self.graph();
        let mut session = Session::new(self.inner.device.clone());
        let mut publish = self.inner.publish.subscribe();
        let mut gate_changes = self.inner.gate_changes.subscribe();
        let mut presence_changes = self.subscribe_presence();
        let mut link = Link {
            peer: self,
            id: self.inner.next_link.fetch_add(1, Ordering::Relaxed),
            device: None,
        };
        let mut reader = FrameReader::new();
        let mut buf = vec![0u8; 64 * 1024];
        write(&mut send, &session.hello(graph)).await?;

        loop {
            // Edits held back go anyway when they have waited long enough.
            let deadline = self.gate().deadline;
            let due = async {
                match deadline {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending().await,
                }
            };
            let read = async {
                match idle {
                    Some(d) => tokio::time::timeout(d, recv.read(&mut buf)).await.ok(),
                    None => Some(recv.read(&mut buf).await),
                }
            };
            tokio::select! {
                read = read => {
                    let Some(read) = read else { return Ok(()) };
                    let Some(n) = read.map_err(read_error)? else { return Ok(()) };
                    reader.push(&buf[..n]);
                    while let Some(frame) = reader.next_frame()? {
                        let message = match Message::decode(&frame) {
                            Ok(m) => m,
                            // From a newer peer; ignore it.
                            Err(DecodeError::UnknownType(_)) => continue,
                            Err(e) => return Err(e.into()),
                        };
                        if let Message::Presence(record) = message {
                            if let Some(device) = &link.device {
                                self.presence().presence.receive(device, record);
                                self.presence_changed();
                            }
                            continue;
                        }
                        let carries_graph = matches!(message, Message::Updates(_) | Message::Snapshot(_));
                        for reply in session.receive(graph, message, Timestamp::now())? {
                            write(&mut send, &reply).await?;
                        }
                        match session.state() {
                            State::Incompatible { theirs } => return Err(Error::Incompatible(*theirs)),
                            State::OtherGraph { theirs } => {
                                let ours = graph
                                    .graph_id()
                                    .map_or_else(|| "none".to_owned(), |id| id.to_string());
                                self.record(
                                    Level::Problem,
                                    session.peer(),
                                    format!("the peer's graph is {theirs}, this device's is {ours}"),
                                );
                                return Err(Error::OtherGraph);
                            }
                            State::Behind => return Err(Error::Behind),
                            State::PeerBehind => return Err(Error::PeerBehind),
                            State::Starting | State::Syncing => {}
                        }
                        if link.device.is_none()
                            && *session.state() == State::Syncing
                            && let Some(device) = session.peer()
                        {
                            self.connected(device);
                            link.device = Some(device.to_owned());
                        }
                        self.note_sync(&link, &session);
                        if carries_graph {
                            self.changed();
                        }
                    }
                }
                changed = presence_changes.changed() => {
                    if changed.is_err() {
                        return Ok(());
                    }
                    if let Some(device) = &link.device {
                        let records = self.presence().presence.outgoing(device);
                        for record in records {
                            write(&mut send, &Message::Presence(record)).await?;
                        }
                    }
                }
                changed = publish.changed() => {
                    if changed.is_err() {
                        return Ok(());
                    }
                    if let Some(message) = session.flush(graph)? {
                        write(&mut send, &message).await?;
                    }
                    self.note_sync(&link, &session);
                }
                changed = gate_changes.changed() => {
                    // Only the deadline moved: loop to read it again.
                    if changed.is_err() {
                        return Ok(());
                    }
                }
                () = due => self.publish_if_due(),
            }
        }
    }
}

/// How this device stands with the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncState {
    /// No other device is listed, and none is connected.
    Alone,
    /// Other devices are listed, and none is connected.
    Offline,
    /// Connected, but this device lacks something a peer has, or has typing
    /// held back that its peers don't have yet.
    Behind,
    /// Connected, and nothing is outstanding.
    Synced,
    /// As `Synced`, and another device has a block open.
    Present,
}

/// What [`Peer::sync_state`] was worked out from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncDetails {
    pub state: SyncState,
    /// Other devices the graph lists, connected or not.
    pub listed: usize,
    /// The devices connected, by ID, in order.
    pub connected: Vec<ConnectedDevice>,
    /// Blocks with typing held back until someone can see it.
    pub held: usize,
    /// Connected devices, or devices they relay for, with a block open.
    pub watching: usize,
}

/// A device this one is connected to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectedDevice {
    pub device: String,
    /// Whether it is known to have something this device lacks.
    pub ahead: bool,
}

impl SyncState {
    /// The state as one lower-case word, for output that a program reads.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Alone => "alone",
            Self::Offline => "offline",
            Self::Behind => "behind",
            Self::Synced => "synced",
            Self::Present => "present",
        }
    }
}

/// [`SyncState`] from what is known: whether the graph lists other devices,
/// what each connected peer is known to have, what this device has, whether
/// typing is held back, and whether another device has a block open.
///
/// Out of date is about what this device is missing or has not sent on
/// purpose. It is not "the versions differ": saving records this device in
/// the graph without sending it, so they would differ for ever.
fn classify(
    others_listed: bool,
    theirs: &[Version],
    ours: &Version,
    held: bool,
    watched: bool,
) -> SyncState {
    if theirs.is_empty() {
        return if others_listed {
            SyncState::Offline
        } else {
            SyncState::Alone
        };
    }
    if held || theirs.iter().any(|v| !ours.includes(v)) {
        SyncState::Behind
    } else if watched {
        SyncState::Present
    } else {
        SyncState::Synced
    }
}

/// The devices connected, in order of ID, from what each connection's peer is
/// known to have. A device with two connections (both dialled at once) is one
/// device, and ahead if either connection's peer is.
fn connected_devices(links: &[(String, Version)], ours: &Version) -> Vec<ConnectedDevice> {
    let mut ahead: BTreeMap<&str, bool> = BTreeMap::new();
    for (device, version) in links {
        *ahead.entry(device).or_default() |= !ours.includes(version);
    }
    ahead
        .into_iter()
        .map(|(device, ahead)| ConnectedDevice {
            device: device.to_owned(),
            ahead,
        })
        .collect()
}

fn bump(counter: &watch::Sender<u64>) {
    counter.send_modify(|n| *n = n.wrapping_add(1));
}

async fn write(send: &mut SendStream, message: &Message) -> Result<(), Error> {
    let mut framed = Vec::new();
    write_frame(&mut framed, &message.encode());
    send.write_all(&framed).await?;
    Ok(())
}

fn close(connection: &Connection, result: &Result<(), Error>) {
    match result {
        Err(Error::PeerBehind) => connection.close(BEHIND.into(), b"behind this device's history"),
        Err(Error::OtherGraph) => connection.close(OTHER_GRAPH.into(), b"another graph"),
        _ => connection.close(0u32.into(), b"done"),
    }
}

/// A peer that found this device behind its history closes with
/// [`BEHIND`], and one that holds another graph with [`OTHER_GRAPH`]: report
/// that, not the lost connection.
fn read_error(e: ReadError) -> Error {
    match e {
        ReadError::ConnectionLost(ConnectionError::ApplicationClosed(close))
            if close.error_code == OTHER_GRAPH.into() =>
        {
            Error::OtherGraph
        }
        ReadError::ConnectionLost(ConnectionError::ApplicationClosed(close))
            if close.error_code == BEHIND.into() =>
        {
            Error::Behind
        }
        e => e.into(),
    }
}

/// Why an endpoint could not be started.
pub type NetError = Box<dyn std::error::Error + Send + Sync>;

/// Binds an endpoint with `key`. With `local` it listens on that address
/// only, with no relays or address lookup: for a local network, or testing.
pub async fn bind(key: SecretKey, local: Option<SocketAddr>) -> Result<Endpoint, NetError> {
    debug!(?local, "binding an endpoint");
    let builder = match local {
        Some(addr) => Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .bind_addr(addr)?,
        None => Endpoint::builder(presets::N0),
    };
    Ok(builder
        .secret_key(key)
        .alpns(vec![ALPN.to_vec(), blobs::ALPN.to_vec()])
        .bind()
        .await?)
}

/// A peer on the network: an endpoint that serves it and, unless it is
/// local, dials the devices its graph lists. Dropping it stops dialling;
/// [`Server::shutdown`] closes the endpoint too.
#[derive(Debug)]
pub struct Server {
    router: Router,
    dialler: Option<tokio::task::JoinHandle<()>>,
    fetcher: Option<tokio::task::JoinHandle<()>>,
}

impl Server {
    #[must_use]
    pub fn endpoint(&self) -> &Endpoint {
        self.router.endpoint()
    }

    /// Closes the endpoint, and with it every connection.
    pub async fn shutdown(self) -> Result<(), NetError> {
        Ok(self.router.shutdown().await?)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        for task in [&self.dialler, &self.fetcher].into_iter().flatten() {
            task.abort();
        }
    }
}

/// Starts `peer` on the network with this device's `key`. See [`bind`] for
/// `local`; with it, devices can't be found from their IDs, so none is
/// dialled and they must dial in.
pub async fn serve(
    peer: &Peer,
    key: SecretKey,
    local: Option<SocketAddr>,
) -> Result<Server, NetError> {
    serve_at(peer, key, local, HashMap::new()).await
}

/// [`serve`], told where some devices are. Where there is no address lookup
/// (`local`) that is the only way to dial one, so with any `known` the
/// listed devices are dialled even then; the rest, if there is a lookup,
/// are found from their IDs as usual. Blobs are fetched from them the same
/// way.
#[expect(
    clippy::implicit_hasher,
    reason = "callers build the map with the default hasher"
)]
pub async fn serve_at(
    peer: &Peer,
    key: SecretKey,
    local: Option<SocketAddr>,
    known: HashMap<EndpointId, SocketAddr>,
) -> Result<Server, NetError> {
    let mut router = Router::builder(bind(key, local).await?).accept(ALPN, peer.clone());
    let blobs = peer.blobs();
    if let Some(blobs) = &blobs {
        let gate = peer.clone();
        router = router.accept(blobs::ALPN, blobs.provider(move |id| gate.permits(id)));
    }
    let router = router.spawn();
    let known = Arc::new(known);
    let dialler = (local.is_none() || !known.is_empty()).then(|| {
        let (peer, endpoint, known) = (peer.clone(), router.endpoint().clone(), Arc::clone(&known));
        tokio::spawn(async move {
            peer.dial_listed(&endpoint, move |id| address_in(&known, id))
                .await;
        })
    });
    let fetcher = blobs.map(|blobs| {
        let (peer, endpoint, known) = (peer.clone(), router.endpoint().clone(), Arc::clone(&known));
        tokio::spawn(async move {
            peer.fetch_blobs(&endpoint, &blobs, move |id| address_in(&known, id))
                .await;
        })
    });
    Ok(Server {
        router,
        dialler,
        fetcher,
    })
}

/// Where a device is: the address it was told it is at, or, with none, just
/// its ID, which needs an address lookup to be dialled.
fn address_in(known: &HashMap<EndpointId, SocketAddr>, id: EndpointId) -> EndpointAddr {
    let addr = EndpointAddr::new(id);
    match known.get(&id) {
        Some(at) => addr.with_ip_addr(*at),
        None => addr,
    }
}

/// Connects to a device for its blobs and fetches one.
async fn fetch_from(
    endpoint: &Endpoint,
    blobs: &blobs::Blobs,
    addr: EndpointAddr,
    hash: blobs::Hash,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connection = tokio::time::timeout(HANDSHAKE, endpoint.connect(addr, blobs::ALPN))
        .await
        .map_err(|_| Error::TimedOut)??;
    let fetched = blobs.fetch(connection.clone(), hash).await;
    connection.close(0u32.into(), b"done");
    Ok(fetched?)
}

/// Reads where devices are, written `ID=ADDRESS` and separated by commas,
/// such as `3f2a…=127.0.0.1:7701,9c1d…=192.168.1.20:7701`.
pub fn known_addresses(text: &str) -> Result<HashMap<EndpointId, SocketAddr>, String> {
    let mut known = HashMap::new();
    for entry in text.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let (id, at) = entry
            .split_once('=')
            .ok_or_else(|| format!("{entry:?} is not ID=ADDRESS"))?;
        let id: EndpointId = id
            .trim()
            .parse()
            .map_err(|_| format!("{id:?} is not a device ID"))?;
        let at: SocketAddr = at
            .trim()
            .parse()
            .map_err(|_| format!("{at:?} is not an address"))?;
        known.insert(id, at);
    }
    Ok(known)
}

/// Saves the graph shortly after it changes, batching bursts of changes.
/// Duplicate journals that arrived from different devices are merged first,
/// and the merge is sent on to connected devices. Aborting the handle
/// it returns stops it, and lets the peer go.
#[allow(
    clippy::must_use_candidate,
    reason = "the command line saves for as long as it runs, and never stops"
)]
pub fn save_on_change(peer: Peer, store: store::Store) -> tokio::task::AbortHandle {
    let mut changes = peer.subscribe();
    let task = tokio::spawn(async move {
        while changes.changed().await.is_ok() {
            tokio::time::sleep(Duration::from_millis(500)).await;
            changes.mark_unchanged();
            let saved = (|| -> Result<(), Box<dyn std::error::Error>> {
                peer.tidy()?;
                store.save(peer.graph())?;
                Ok(())
            })();
            if let Err(e) = saved {
                error!(error = %e, "saving failed");
            }
        }
    });
    task.abort_handle()
}

impl ProtocolHandler for Peer {
    #[instrument(level = "debug", skip_all, fields(remote = %connection.remote_id().fmt_short()))]
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let device = connection.remote_id().to_string();
        if !self.permits(&connection.remote_id()) {
            warn!("refusing a device that isn't allowed: see `allow`");
            self.record(
                Level::Problem,
                Some(&device),
                "refused a connection: the device isn't listed in the graph or let in".to_owned(),
            );
            connection.close(1u32.into(), b"not allowed");
            return Ok(());
        }
        self.record(
            Level::Info,
            Some(&device),
            "accepted a connection".to_owned(),
        );
        let result = async {
            let (send, recv) = connection.accept_bi().await?;
            self.sync(send, recv, None).await
        }
        .await;
        close(&connection, &result);
        self.record_end(&device, "accepted connection", &result);
        match result {
            // A phone that sleeps or leaves the network never says goodbye, and
            // iroh would warn a second time if we returned the error.
            Err(e) if peer_went_away(&e) => {
                info!(error = %e, "sync ended: the peer went away");
                Ok(())
            }
            Err(e) => {
                warn!(error = %e, "sync ended");
                Err(AcceptError::from_err(e))
            }
            Ok(()) => Ok(()),
        }
    }
}

/// Why the other device closed the connection, if it did so deliberately and
/// not as a goodbye: a refusal, or one of the reasons in [`close`].
fn refused_by_peer(e: &Error) -> Option<String> {
    let (Error::Connection(ConnectionError::ApplicationClosed(closed))
    | Error::Read(ReadError::ConnectionLost(ConnectionError::ApplicationClosed(closed)))
    | Error::Write(WriteError::ConnectionLost(ConnectionError::ApplicationClosed(closed)))) = e
    else {
        return None;
    };
    (closed.error_code != 0u32.into()).then(|| String::from_utf8_lossy(&closed.reason).into_owned())
}

/// Whether a sync ended because the connection did, which is the peer's doing
/// and nothing to act on, rather than because of something said on it.
fn peer_went_away(e: &Error) -> bool {
    matches!(
        e,
        Error::Connection(_)
            | Error::Read(ReadError::ConnectionLost(_))
            | Error::Write(WriteError::ConnectionLost(_))
    )
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    #[test]
    fn known_addresses_are_read_from_id_equals_address() {
        let one = SecretKey::generate().public();
        let two = SecretKey::generate().public();
        let known = known_addresses(&format!(" {one}=127.0.0.1:7701 , {two}=[::1]:9 ,")).unwrap();
        assert_eq!(known.len(), 2);
        assert_eq!(known[&one], "127.0.0.1:7701".parse().unwrap());
        assert_eq!(known[&two], "[::1]:9".parse().unwrap());
        assert!(known_addresses("").unwrap().is_empty());

        // Each way it can be wrong says which part.
        assert!(
            known_addresses("127.0.0.1:7701")
                .unwrap_err()
                .contains("ID=ADDRESS")
        );
        assert!(
            known_addresses("nobody=127.0.0.1:1")
                .unwrap_err()
                .contains("device ID")
        );
        assert!(
            known_addresses(&format!("{one}=here"))
                .unwrap_err()
                .contains("address")
        );
    }

    #[test]
    fn a_lost_connection_is_the_peer_going_away_and_nothing_else_is() {
        let gone = ConnectionError::TimedOut;
        assert!(peer_went_away(&Error::Connection(gone.clone())));
        assert!(peer_went_away(&Error::Read(ReadError::ConnectionLost(
            gone.clone()
        ))));
        assert!(peer_went_away(&Error::Write(WriteError::ConnectionLost(
            gone
        ))));

        // What was said on a connection that held is for someone to look at.
        assert!(!peer_went_away(&Error::TimedOut));
        assert!(!peer_went_away(&Error::Incompatible(None)));
        assert!(!peer_went_away(&Error::OtherGraph));
        assert!(!peer_went_away(&Error::PeerBehind));
        assert!(!peer_went_away(&Error::Read(ReadError::ClosedStream)));
    }

    /// How a connection ended is put in the log by what it was: a peer going
    /// away is routine, what was said on a connection that held is not.
    #[test]
    fn how_a_connection_ended_is_logged_by_what_it_was() {
        let peer = Peer::new(Graph::new(), Allowed::Any, "me");
        peer.record_end("a", "dialled connection", &Ok(()));
        peer.record_end(
            "b",
            "dialled connection",
            &Err(Error::Connection(ConnectionError::TimedOut)),
        );
        peer.record_end("c", "accepted connection", &Err(Error::OtherGraph));
        let log: Vec<_> = peer
            .events()
            .into_iter()
            .map(|e| (e.level, e.device.unwrap(), e.message))
            .collect();
        assert_eq!(
            log,
            [
                (
                    Level::Info,
                    "a".to_owned(),
                    "dialled connection ended".to_owned()
                ),
                (
                    Level::Info,
                    "b".to_owned(),
                    "dialled connection ended: the connection was lost (timed out)".to_owned()
                ),
                (
                    Level::Problem,
                    "c".to_owned(),
                    "accepted connection ended: the peer holds another graph, and this device won't merge two"
                        .to_owned()
                ),
            ]
        );
    }

    #[test]
    fn a_peer_knows_its_own_device() {
        assert_eq!(Peer::new(Graph::new(), Allowed::Any, "me").device(), "me");
    }

    /// A version `graph` has, and the other doesn't.
    fn version_of(text: &str) -> Version {
        let graph = Graph::new();
        graph.create_block(None, None, text).unwrap();
        graph.version()
    }

    #[test]
    fn the_state_follows_who_is_listed_connected_and_missing() {
        let ours = version_of("ours");
        let theirs = version_of("theirs");
        let state = |others_listed, links: &[Version], held, watched| {
            classify(others_listed, links, &ours, held, watched)
        };

        // Nobody connected: whether anyone is listed is all there is to say.
        assert_eq!(state(false, &[], false, false), SyncState::Alone);
        assert_eq!(state(true, &[], false, false), SyncState::Offline);
        assert_eq!(state(true, &[], true, true), SyncState::Offline);

        // Connected, with nothing outstanding. Who is listed no longer matters.
        assert_eq!(
            state(false, std::slice::from_ref(&ours), false, false),
            SyncState::Synced
        );
        // A peer with less than we have isn't out of date: the rest is on its
        // way, or is only our record of having saved.
        assert_eq!(
            state(true, &[Version::default()], false, false),
            SyncState::Synced
        );

        // A peer with something we lack: we are behind, whichever else.
        assert_eq!(
            state(true, std::slice::from_ref(&theirs), false, false),
            SyncState::Behind
        );
        assert_eq!(
            state(true, &[ours.clone(), theirs.clone()], false, false),
            SyncState::Behind,
            "one is enough"
        );
        // Typing held back is behind as well.
        assert_eq!(
            state(true, std::slice::from_ref(&ours), true, false),
            SyncState::Behind
        );

        // Someone else is here, if nothing is outstanding.
        assert_eq!(
            state(true, std::slice::from_ref(&ours), false, true),
            SyncState::Present
        );
        assert_eq!(state(true, &[theirs], false, true), SyncState::Behind);
        assert_eq!(
            state(true, std::slice::from_ref(&ours), true, true),
            SyncState::Behind
        );
    }

    #[test]
    fn a_device_with_two_connections_is_listed_once_and_ahead_if_either_is() {
        let ours = version_of("ours");
        let theirs = version_of("theirs");
        let link = |device: &str, version: &Version| (device.to_owned(), version.clone());
        let ahead = |links: &[(String, Version)]| -> Vec<(String, bool)> {
            connected_devices(links, &ours)
                .into_iter()
                .map(|c| (c.device, c.ahead))
                .collect()
        };

        assert_eq!(ahead(&[]), []);
        // In order of ID, whichever order the connections are in.
        assert_eq!(
            ahead(&[link("b", &ours), link("a", &theirs)]),
            [("a".to_owned(), true), ("b".to_owned(), false)]
        );
        // One of two connections to a device having more is enough, either way round.
        assert_eq!(
            ahead(&[link("a", &ours), link("a", &theirs)]),
            [("a".to_owned(), true)]
        );
        assert_eq!(
            ahead(&[link("a", &theirs), link("a", &ours)]),
            [("a".to_owned(), true)]
        );
        // And neither is not.
        assert_eq!(
            ahead(&[link("a", &ours), link("a", &Version::default())]),
            [("a".to_owned(), false)]
        );
    }

    /// Sets a flag when dropped, as a task's state is when it is aborted.
    struct Flag(Arc<AtomicBool>);

    impl Drop for Flag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    /// Dropping the server stops the dialler that goes with it.
    #[tokio::test]
    async fn dropping_the_server_stops_its_dialler() {
        let local = Some(SocketAddr::from(([127, 0, 0, 1], 0)));
        let router = Router::builder(bind(SecretKey::generate(), local).await.unwrap()).spawn();
        let stopped = Arc::new(AtomicBool::new(false));
        let flag = Flag(Arc::clone(&stopped));
        let dialler = tokio::spawn(async move {
            let _flag = flag;
            std::future::pending::<()>().await;
        });
        tokio::task::yield_now().await;
        assert!(!stopped.load(Ordering::SeqCst), "running until dropped");

        drop(Server {
            router,
            dialler: Some(dialler),
            fetcher: None,
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            while !stopped.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the dialler is aborted");
    }
}
