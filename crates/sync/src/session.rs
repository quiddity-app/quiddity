use jiff::Timestamp;
use quiddity_core::{Graph, GraphId, SCHEMA_VERSION, Version};
use tracing::debug;

use crate::Message;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Graph(#[from] quiddity_core::Error),
    #[error("updates arrived before the peer's hello")]
    UpdatesBeforeHello,
    #[error("a snapshot arrived that this replica can't build on")]
    UnexpectedSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Waiting for the peer's hello.
    Starting,
    Syncing,
    /// The peer's graph uses a schema this build can't read, or ours is one
    /// it can't. Nothing is exchanged; the device that is behind needs
    /// updating.
    Incompatible {
        theirs: Option<i64>,
    },
    /// Both sides know which graph they hold, and it isn't the same one.
    /// Nothing is exchanged: merging two graphs would also merge their device
    /// lists, and so make each one's devices trusted by the other.
    OtherGraph {
        theirs: GraphId,
    },
    /// This replica is behind where the peer's history starts, so the peer
    /// can't bring it up to date, and edits made here may rely on history
    /// the peer has dropped. Nothing is exchanged: this replica was evicted,
    /// and needs to start again from a snapshot.
    Behind,
    /// The peer is behind where this replica's history starts. Nothing is
    /// exchanged; the peer needs to start again from a snapshot.
    PeerBehind,
}

/// One side of a sync connection with one peer.
#[derive(Debug)]
pub struct Session {
    /// This device's ID in the graph's device list.
    device: String,
    /// The peer's device ID, once its hello has arrived.
    peer: Option<String>,
    state: State,
    /// Everything the peer is known to have: what it said it had, plus
    /// everything sent to it or received from it since.
    theirs: Version,
}

impl Session {
    pub fn new(device: impl Into<String>) -> Self {
        Self {
            device: device.into(),
            peer: None,
            state: State::Starting,
            theirs: Version::default(),
        }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    /// Everything the peer is known to have: what it said it had, plus
    /// everything sent to it or received from it since.
    pub fn theirs(&self) -> &Version {
        &self.theirs
    }

    /// The peer's device ID, once its hello has arrived.
    pub fn peer(&self) -> Option<&str> {
        self.peer.as_deref()
    }

    /// The message to send as soon as the connection is up.
    pub fn hello(&self, graph: &Graph) -> Message {
        Message::Hello {
            schema: graph.schema_version(),
            graph: graph.graph_id(),
            device: self.device.clone(),
            start: graph.history_start(),
            version: graph.version(),
        }
    }

    /// Handles a message from the peer, returning any replies. After
    /// updates are applied, the caller should flush its other sessions so
    /// the updates travel on.
    ///
    /// A replica with anything to give vouches for a peer it hasn't heard
    /// of (see `Graph::vouch_for`), at `now`, so no device compacts past
    /// what the peer has.
    pub fn receive(
        &mut self,
        graph: &Graph,
        message: Message,
        now: Timestamp,
    ) -> Result<Vec<Message>, Error> {
        match message {
            Message::Hello {
                schema,
                graph: theirs,
                device,
                start,
                version,
            } => {
                self.peer = Some(device.clone());
                let readable = |s: Option<i64>| s.is_none_or(|s| s == SCHEMA_VERSION);
                if !readable(schema) || !readable(graph.schema_version()) {
                    debug!(%device, ?schema, "refusing a peer with another schema");
                    self.state = State::Incompatible { theirs: schema };
                    return Ok(Vec::new());
                }
                // A replica that doesn't know its graph's ID (a blank one, or
                // one from before graphs had them) takes the peer's word.
                if let (Some(ours), Some(theirs)) = (graph.graph_id(), theirs)
                    && ours != theirs
                {
                    debug!(%device, %theirs, "refusing a peer with another graph");
                    self.state = State::OtherGraph { theirs };
                    return Ok(Vec::new());
                }
                let ours = graph.version();
                let blank = |v: &Version| *v == Version::default();
                // A blank replica is behind everyone, but can start from a
                // snapshot; any other is stranded.
                if !blank(&ours) && !ours.includes(&start) {
                    debug!(%device, "this replica is behind the peer's history");
                    self.state = State::Behind;
                    return Ok(Vec::new());
                }
                if !blank(&version) && !graph.can_update(&version) {
                    debug!(%device, "the peer is behind this replica's history");
                    self.state = State::PeerBehind;
                    return Ok(Vec::new());
                }
                if !blank(&ours) {
                    graph.vouch_for(&device, &version, now)?;
                }
                debug!(%device, "syncing");
                self.state = State::Syncing;
                self.theirs = version;
                Ok(self.flush(graph)?.into_iter().collect())
            }
            // Presence is a device's business, not a session's: the caller
            // routes it between connections.
            Message::Presence(_) => Ok(Vec::new()),
            Message::Updates(_) | Message::Snapshot(_) if self.state == State::Starting => {
                Err(Error::UpdatesBeforeHello)
            }
            // Refused at the hello: nothing is exchanged.
            Message::Updates(_) | Message::Snapshot(_) if self.state != State::Syncing => {
                Ok(Vec::new())
            }
            Message::Updates(updates) => {
                debug!(bytes = updates.len(), "importing updates");
                graph.import(&updates)?;
                self.theirs.merge(&Graph::version_of_updates(&updates)?);
                Ok(Vec::new())
            }
            // A peer that can't send updates sends its snapshot: to a blank
            // replica, or, if its history has moved on since the hello, to
            // one that has reached where it now starts. Either can build on
            // it; anything else would import nothing, silently.
            Message::Snapshot(snapshot) => {
                debug!(bytes = snapshot.len(), "importing a snapshot");
                let ours = graph.version();
                if ours != Version::default() && !ours.includes(&Graph::start_of(&snapshot)?) {
                    return Err(Error::UnexpectedSnapshot);
                }
                graph.import(&snapshot)?;
                // What the snapshot brings, not all this replica has: it may
                // already have had edits the peer hasn't.
                self.theirs.merge(&Graph::version_of_updates(&snapshot)?);
                Ok(Vec::new())
            }
        }
    }

    /// What the peer is missing, if anything: updates, or a snapshot if
    /// this replica can't bring the peer up to date with updates (a blank
    /// peer, or one this replica has compacted past since the hello). Call
    /// after the graph changes.
    pub fn flush(&mut self, graph: &Graph) -> Result<Option<Message>, Error> {
        let ours = graph.version();
        if self.state != State::Syncing || self.theirs.includes(&ours) {
            return Ok(None);
        }
        let message = if graph.can_update(&self.theirs) {
            Message::Updates(graph.updates_since(&self.theirs)?)
        } else {
            Message::Snapshot(graph.snapshot()?)
        };
        self.theirs.merge(&ours);
        Ok(Some(message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Convergence over a network, compacted peers included, is
    // property-tested in tests/simulation.rs. These cover the refusals and
    // the hello's side effects.

    fn now() -> Timestamp {
        "2026-10-26T12:00:00Z".parse().unwrap()
    }

    fn hello(schema: Option<i64>, graph: &Graph) -> Message {
        Message::Hello {
            schema,
            graph: graph.graph_id(),
            device: "peer".into(),
            start: graph.history_start(),
            version: graph.version(),
        }
    }

    fn updates_from_elsewhere() -> Vec<u8> {
        let other = Graph::new();
        other.create_block(None, None, "elsewhere").unwrap();
        other.updates_since(&Version::default()).unwrap()
    }

    /// `a`, with history compacted past where `behind` is.
    fn compacted_past() -> (Graph, Graph) {
        let a = Graph::new();
        let behind = Graph::from_snapshot(&a.snapshot().unwrap()).unwrap();
        a.create_block(None, None, "later").unwrap();
        a.commit();
        let compacted = Graph::from_snapshot(&a.shallow_snapshot(&a.version()).unwrap()).unwrap();
        (compacted, behind)
    }

    #[test]
    fn refuses_a_schema_it_cannot_read() -> Result<(), Error> {
        let graph = Graph::new();
        let mut session = Session::new("me");
        let newer = hello(Some(SCHEMA_VERSION + 1), &Graph::blank());
        assert!(session.receive(&graph, newer, now())?.is_empty());
        assert_eq!(
            session.state(),
            &State::Incompatible {
                theirs: Some(SCHEMA_VERSION + 1)
            }
        );
        // Nothing goes either way.
        graph.create_block(None, None, "local")?;
        assert!(session.flush(&graph)?.is_none());
        session.receive(&graph, Message::Updates(updates_from_elsewhere()), now())?;
        assert_eq!(graph.roots().len(), 1);
        Ok(())
    }

    /// A hello for a replica of `graph` that says its graph is `id`.
    fn hello_as(id: Option<GraphId>, graph: &Graph) -> Message {
        match hello(graph.schema_version(), graph) {
            Message::Hello {
                schema,
                device,
                start,
                version,
                ..
            } => Message::Hello {
                schema,
                graph: id,
                device,
                start,
                version,
            },
            other => other,
        }
    }

    #[test]
    fn refuses_another_graph_and_exchanges_nothing() -> Result<(), Error> {
        let (ours, theirs) = (Graph::new(), Graph::new());
        ours.create_block(None, None, "local")?;
        theirs.create_block(None, None, "theirs")?;
        assert_ne!(ours.graph_id(), theirs.graph_id());

        let mut session = Session::new("me");
        assert!(
            session
                .receive(&ours, hello(theirs.schema_version(), &theirs), now())?
                .is_empty()
        );
        assert_eq!(
            session.state(),
            &State::OtherGraph {
                theirs: theirs.graph_id().unwrap()
            }
        );
        // Nor does it vouch for the peer, which would list it as holding ours.
        assert!(ours.devices().is_empty());
        ours.create_block(None, None, "more")?;
        assert!(session.flush(&ours)?.is_none());
        session.receive(&ours, Message::Updates(updates_from_elsewhere()), now())?;
        assert_eq!(ours.roots().len(), 2);
        Ok(())
    }

    #[test]
    fn the_same_graph_syncs() -> Result<(), Error> {
        let ours = Graph::new();
        let theirs = Graph::from_snapshot(&ours.snapshot()?)?;
        theirs.create_block(None, None, "theirs")?;
        let mut session = Session::new("me");
        session.receive(&ours, hello(theirs.schema_version(), &theirs), now())?;
        assert_eq!(session.state(), &State::Syncing);
        Ok(())
    }

    /// A replica that doesn't know its graph's ID, or a peer that doesn't,
    /// is not refused: a blank replica is joining, and a graph from before
    /// graphs had IDs is the same graph as the one that has been given one.
    #[test]
    fn a_side_that_does_not_know_its_graph_is_not_refused() -> Result<(), Error> {
        let known = Graph::new();
        let mut blank_side = Session::new("me");
        blank_side.receive(
            &Graph::blank(),
            hello(known.schema_version(), &known),
            now(),
        )?;
        assert_eq!(blank_side.state(), &State::Syncing);

        let mut unknown_peer = Session::new("me");
        unknown_peer.receive(&known, hello_as(None, &known), now())?;
        assert_eq!(unknown_peer.state(), &State::Syncing);

        let mut neither = Session::new("me");
        neither.receive(&Graph::blank(), hello_as(None, &Graph::blank()), now())?;
        assert_eq!(neither.state(), &State::Syncing);
        Ok(())
    }

    #[test]
    fn says_which_graph_it_holds() {
        let graph = Graph::new();
        let Message::Hello { graph: id, .. } = Session::new("me").hello(&graph) else {
            panic!("not a hello");
        };
        assert_eq!(id, graph.graph_id());
        assert!(id.is_some());
        let Message::Hello { graph: id, .. } = Session::new("me").hello(&Graph::blank()) else {
            panic!("not a hello");
        };
        assert_eq!(id, None);
    }

    /// Updates from another replica that edits as the same node are refused, and
    /// the session says so rather than applying some of them: the replicas would
    /// be left different, each believing they agree.
    #[test]
    fn updates_from_a_twin_are_an_error_and_apply_nothing() -> Result<(), Error> {
        use quiddity_core::NodeId;
        let node = NodeId::new(5);
        let base = Graph::new_as(node);
        base.create_block(None, None, "base")?;
        base.commit();
        let (ours, theirs) = (
            Graph::from_snapshot(&base.snapshot()?)?,
            Graph::from_snapshot(&base.snapshot()?)?,
        );
        for graph in [&ours, &theirs] {
            graph.set_node_id(node)?;
        }
        ours.create_block(None, None, "ours")?;
        for text in ["theirs 1", "theirs 2"] {
            theirs.create_block(None, None, text)?;
        }
        ours.commit();
        theirs.commit();

        let mut session = Session::new("me");
        session.receive(&ours, hello(theirs.schema_version(), &theirs), now())?;
        assert_eq!(session.state(), &State::Syncing);
        let before = ours.roots().len();
        let updates = Message::Updates(theirs.updates_since(&ours.version())?);
        let error = session.receive(&ours, updates, now()).unwrap_err();
        assert!(
            matches!(error, Error::Graph(quiddity_core::Error::Twin(n)) if n == node),
            "{error:?}"
        );
        assert_eq!(ours.roots().len(), before, "nothing was applied");
        assert_ne!(ours.node_id(), node, "and it stopped editing as the node");
        Ok(())
    }

    #[test]
    fn a_blank_device_accepts_any_readable_graph() -> Result<(), Error> {
        let mut session = Session::new("me");
        session.receive(
            &Graph::blank(),
            hello(Some(SCHEMA_VERSION), &Graph::blank()),
            now(),
        )?;
        assert_eq!(session.state(), &State::Syncing);
        Ok(())
    }

    #[test]
    fn what_the_peer_has_grows_with_what_is_exchanged() -> Result<(), Error> {
        let (graph, mut session) = (Graph::blank(), Session::new("me"));
        assert_eq!(session.theirs(), &Version::default());
        let other = Graph::new();
        other.create_block(None, None, "theirs")?;
        let mut hello = hello(None, &graph);
        if let Message::Hello { version, .. } = &mut hello {
            *version = other.version();
        }
        session.receive(&graph, hello, now())?;
        assert_eq!(session.theirs(), &other.version(), "what its hello said");

        // Sending them what we have adds to it.
        graph.create_block(None, None, "ours")?;
        assert!(session.flush(&graph)?.is_some());
        assert!(session.theirs().includes(&graph.version()));
        assert!(session.theirs().includes(&other.version()));
        Ok(())
    }

    #[test]
    fn the_peer_is_known_from_its_hello() -> Result<(), Error> {
        let mut session = Session::new("me");
        assert_eq!(session.peer(), None);
        let graph = Graph::new();
        session.receive(&graph, hello(None, &graph), now())?;
        assert_eq!(session.peer(), Some("peer"));
        Ok(())
    }

    #[test]
    fn presence_is_left_to_the_caller() -> Result<(), Error> {
        let (graph, mut session) = (Graph::new(), Session::new("me"));
        let record = Message::Presence(crate::Record {
            device: "peer".into(),
            epoch: 1,
            seq: 1,
            peers: Default::default(),
            focus: Default::default(),
        });
        // Not an error before the hello, and no reply either way.
        assert!(session.receive(&graph, record.clone(), now())?.is_empty());
        assert_eq!(session.state(), &State::Starting);
        session.receive(&graph, hello(None, &graph), now())?;
        assert!(session.receive(&graph, record, now())?.is_empty());
        Ok(())
    }

    #[test]
    fn updates_need_a_hello_first() {
        let mut session = Session::new("me");
        let updates = Message::Updates(updates_from_elsewhere());
        let result = session.receive(&Graph::new(), updates, now());
        assert!(matches!(result, Err(Error::UpdatesBeforeHello)));
    }

    /// An evicted device and a peer compacted past it each find the other
    /// behind, and exchange nothing.
    #[test]
    fn neither_side_sends_what_can_never_apply() -> Result<(), Error> {
        let (compacted, behind) = compacted_past();
        behind.create_block(None, None, "made on dropped history")?;
        behind.commit();

        let mut ahead = Session::new("ahead");
        ahead.receive(
            &compacted,
            hello(compacted.schema_version(), &behind),
            now(),
        )?;
        assert_eq!(ahead.state(), &State::PeerBehind);
        assert!(ahead.flush(&compacted)?.is_none());

        let mut stranded = Session::new("behind");
        stranded.receive(
            &behind,
            hello(compacted.schema_version(), &compacted),
            now(),
        )?;
        assert_eq!(stranded.state(), &State::Behind);
        assert!(stranded.flush(&behind)?.is_none());
        let roots = behind.roots().len();
        stranded.receive(&behind, Message::Updates(compacted.snapshot()?), now())?;
        assert_eq!(behind.roots().len(), roots);
        Ok(())
    }

    /// A blank device can't be brought up to date by a compacted peer's
    /// updates, so it is sent a snapshot, and syncs as usual after that.
    #[test]
    fn a_blank_device_starts_from_a_snapshot() -> Result<(), Error> {
        let (compacted, _) = compacted_past();
        let blank = Graph::blank();
        let mut ahead = Session::new("ahead");
        let mut joining = Session::new("joining");
        let to_blank = ahead.receive(&compacted, joining.hello(&blank), now())?;
        assert!(matches!(to_blank.as_slice(), [Message::Snapshot(_)]));
        joining.receive(&blank, ahead.hello(&compacted), now())?;
        for message in to_blank {
            joining.receive(&blank, message, now())?;
        }
        assert_eq!(blank.version(), compacted.version());

        compacted.create_block(None, None, "after joining")?;
        compacted.commit();
        let Some(next) = ahead.flush(&compacted)? else {
            panic!("nothing sent")
        };
        assert!(matches!(next, Message::Updates(_)));
        joining.receive(&blank, next, now())?;
        assert_eq!(blank.version(), compacted.version());
        Ok(())
    }

    /// A snapshot is only taken by a replica that can build on it.
    #[test]
    fn a_snapshot_must_build_on_what_is_here() -> Result<(), Error> {
        let (compacted, behind) = compacted_past();
        let mut session = Session::new("me");
        session.receive(&behind, hello(behind.schema_version(), &behind), now())?;
        let result = session.receive(&behind, Message::Snapshot(compacted.snapshot()?), now());
        assert!(matches!(result, Err(Error::UnexpectedSnapshot)));
        Ok(())
    }

    #[test]
    fn devices_vouch_for_newcomers_they_can_give_to() -> Result<(), Error> {
        let (graph, blank) = (Graph::new(), Graph::blank());
        let newcomer = Graph::blank();
        Session::new("me").receive(&graph, hello(None, &newcomer), now())?;
        let devices = graph.devices();
        assert_eq!(devices.len(), 1);
        assert_eq!(
            (devices[0].id.as_str(), &devices[0].stored),
            ("peer", &Version::default())
        );

        Session::new("me").receive(&blank, hello(Some(SCHEMA_VERSION), &graph), now())?;
        assert!(
            blank.devices().is_empty(),
            "a blank device has nothing to give"
        );
        Ok(())
    }
}
