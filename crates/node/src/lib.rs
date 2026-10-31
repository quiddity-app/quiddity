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

pub mod store;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use iroh::endpoint::{Connection, ConnectionError, ReadError, RecvStream, SendStream, WriteError};
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh::{Endpoint, EndpointAddr, EndpointId};
use jiff::Timestamp;
use quiddity_core::{Graph, Index, merge_duplicates};
use quiddity_sync::{
    DecodeError, FrameError, FrameReader, Message, Presence, Record, Session, State, write_frame,
};
use tokio::sync::watch;

/// The protocol's name, negotiated when connecting.
pub const ALPN: &[u8] = b"quiddity/sync/2";

/// How long after the last keystroke an edit nobody is watching is sent.
const EDIT_IDLE: Duration = Duration::from_secs(2);

/// The code a device closes a connection with when the peer is behind its
/// history, so the peer can say why rather than that the connection went.
const BEHIND: u32 = 2;

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
    #[error("the peer's graph uses schema {0:?}, which this build can't sync with")]
    Incompatible(Option<i64>),
    #[error("this device is behind the peer's history: it needs to start again from a snapshot")]
    Behind,
    #[error("the peer is behind this device's history: it needs to start again from a snapshot")]
    PeerBehind,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{} already holds a graph", .0.display())]
    Exists(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
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
    if graph.version() == quiddity_core::Version::default() {
        return Err(AllowError::NotJoined);
    }
    graph.vouch_for(device, &quiddity_core::Version::default(), now)?;
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
    /// Bumped whenever the graph changes, so that it is saved.
    changes: watch::Sender<u64>,
    /// Bumped when the changes so far are to be sent, so every connection
    /// flushes. It follows `changes` except while edits are held back.
    publish: watch::Sender<u64>,
    gate: Mutex<Gate>,
    /// Bumped when the gate's deadline moves, so connections re-read it.
    gate_changes: watch::Sender<u64>,
    presence: Mutex<PresenceState>,
    /// Bumped whenever presence changes, so every connection announces it.
    presence_changes: watch::Sender<u64>,
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

#[derive(Debug)]
struct PresenceState {
    presence: Presence,
    /// How many connections each connected device has. Two devices that
    /// dial each other at once have two.
    links: BTreeMap<String, usize>,
}

/// A connection that has been announced as a link in the presence network,
/// and is withdrawn when the connection ends, however it ends.
struct Link<'a> {
    peer: &'a Peer,
    device: Option<String>,
}

impl Drop for Link<'_> {
    fn drop(&mut self) {
        if let Some(device) = &self.device {
            self.peer.disconnected(device);
        }
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
                }),
                presence_changes: watch::Sender::new(0),
                gate: Mutex::new(Gate {
                    pending: BTreeSet::new(),
                    deadline: None,
                    idle: EDIT_IDLE,
                }),
                gate_changes: watch::Sender::new(0),
                publish: watch::Sender::new(0),
                device,
                graph,
                allowed,
                changes: watch::Sender::new(0),
            }),
        }
    }

    pub fn graph(&self) -> &Graph {
        &self.inner.graph
    }

    /// Authorises `device` (an endpoint ID) to connect to every device that
    /// shares this graph, by listing it in the graph.
    pub fn allow(&self, device: &str) -> Result<(), AllowError> {
        allow_device(self.graph(), device, Timestamp::now())?;
        self.changed();
        Ok(())
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
    pub fn watched(&self, block: &str) -> bool {
        self.presence().presence.watched(block)
    }

    /// The other devices that are here, and what they have open.
    pub fn present(&self) -> Vec<Record> {
        let state = self.presence();
        state.presence.present().into_iter().cloned().collect()
    }

    /// Watches for changes to who is here and what they have open.
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
        if self.pending_watched() {
            self.publish_now();
        }
    }

    fn connected(&self, device: &str) {
        let mut state = self.presence();
        *state.links.entry(device.to_owned()).or_default() += 1;
        state.presence.connect(device);
        drop(state);
        self.presence_changed();
    }

    fn disconnected(&self, device: &str) {
        let mut state = self.presence();
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
        self.presence_changed();
    }

    /// Merges duplicate entities that arrived from different devices, and
    /// sends the merge on to connected devices. Returns whether anything
    /// moved.
    pub fn tidy(&self) -> Result<bool, quiddity_core::Error> {
        let graph = self.graph();
        let moved = merge_duplicates(graph, &Index::build(graph)?)? > 0;
        if moved {
            self.changed();
        }
        Ok(moved)
    }

    /// Watches for changes to the graph, local or from peers.
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
        let connection = endpoint.connect(addr, ALPN).await?;
        let (send, recv) = connection.open_bi().await?;
        let result = self.sync(send, recv, idle).await;
        close(&connection, &result);
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
                        let is_updates = matches!(message, Message::Updates(_));
                        for reply in session.receive(graph, message, Timestamp::now())? {
                            write(&mut send, &reply).await?;
                        }
                        match session.state() {
                            State::Incompatible { theirs } => return Err(Error::Incompatible(*theirs)),
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
                        if is_updates {
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
        _ => connection.close(0u32.into(), b"done"),
    }
}

/// A peer that found this device behind its history closes with
/// [`BEHIND`]: report that, not the lost connection.
fn read_error(e: ReadError) -> Error {
    match e {
        ReadError::ConnectionLost(ConnectionError::ApplicationClosed(close))
            if close.error_code == BEHIND.into() =>
        {
            Error::Behind
        }
        e => e.into(),
    }
}

impl ProtocolHandler for Peer {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        if let Allowed::Only(ids) = &self.inner.allowed
            && !ids.contains(&connection.remote_id())
            && !self.listed(&connection.remote_id())
        {
            connection.close(1u32.into(), b"not allowed");
            return Ok(());
        }
        let (send, recv) = connection.accept_bi().await?;
        let result = self.sync(send, recv, None).await;
        close(&connection, &result);
        result.map_err(AcceptError::from_err)
    }
}
