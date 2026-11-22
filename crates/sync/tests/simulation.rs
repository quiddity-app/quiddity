//! Sessions over a simulated network: devices edit, connect and disconnect
//! at random, and messages are delivered in random order across
//! connections (but in order within one, as on a QUIC stream). Messages in
//! flight when a connection drops are lost.
//!
//! Devices batch their outgoing updates, as an app would, so flushing is a
//! step of its own rather than following every change.
//!
//! Devices record what they have stored and compact by the device list,
//! as they would on saving, so sessions meet compacted peers. No session
//! may ever find either side behind the other's history.
//!
//! The run ends with the devices connected only in a chain, so that
//! everything has to be relayed through the middle ones.
#![expect(clippy::unwrap_used, reason = "a test failing to set up should panic")]

use std::collections::VecDeque;

use jiff::{SignedDuration, Timestamp};
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::Index;
use quiddity_core::{BlockId, Graph};
use quiddity_sync::{FrameReader, Message, Session, State, write_frame};

const DEVICES: usize = 3;
const ACTIVE: SignedDuration = SignedDuration::from_hours(24 * 30);

fn now() -> Timestamp {
    "2026-10-26T12:00:00Z".parse().unwrap()
}

/// A device that has yet to join: it has nothing to edit or store.
fn blank(graph: &Graph) -> bool {
    graph.version() == quiddity_core::Version::default()
}

/// One direction of a connection: framed bytes in flight.
#[derive(Debug, Default)]
struct Pipe {
    frames: VecDeque<Vec<u8>>,
}

impl Pipe {
    fn send(&mut self, message: &Message) {
        let mut framed = Vec::new();
        write_frame(&mut framed, &message.encode());
        self.frames.push_back(framed);
    }
}

#[derive(Debug)]
struct Connection {
    ends: [usize; 2],
    sessions: [Session; 2],
    /// `pipes[i]` carries messages towards `ends[i]`.
    pipes: [Pipe; 2],
}

struct Network {
    devices: Vec<Graph>,
    connections: Vec<Connection>,
}

impl Network {
    fn new() -> Self {
        let origin = Graph::new();
        origin.record_stored("0", now()).unwrap();
        let mut devices = vec![origin];
        devices.extend((1..DEVICES).map(|_| Graph::blank()));
        Self {
            devices,
            connections: Vec::new(),
        }
    }

    fn connect(&mut self, a: usize, b: usize) {
        if a == b
            || self
                .connections
                .iter()
                .any(|c| c.ends == [a, b] || c.ends == [b, a])
        {
            return;
        }
        let mut connection = Connection {
            ends: [a, b],
            sessions: [Session::new(a.to_string()), Session::new(b.to_string())],
            pipes: [Pipe::default(), Pipe::default()],
        };
        let hellos = [0, 1].map(|i| connection.sessions[i].hello(&self.devices[[a, b][i]]));
        connection.pipes[1].send(&hellos[0]);
        connection.pipes[0].send(&hellos[1]);
        self.connections.push(connection);
    }

    /// Offers a device's changes to all its peers.
    fn flush(&mut self, device: usize) {
        let graph = &self.devices[device];
        for c in &mut self.connections {
            for side in 0..2 {
                if c.ends[side] == device
                    && let Some(m) = c.sessions[side].flush(graph).unwrap()
                {
                    c.pipes[1 - side].send(&m);
                }
            }
        }
    }

    /// Delivers the next message towards one end of a connection.
    fn deliver(&mut self, connection: usize, to: usize) -> bool {
        let c = &mut self.connections[connection];
        let Some(bytes) = c.pipes[to].frames.pop_front() else {
            return false;
        };
        let mut reader = FrameReader::new();
        reader.push(&bytes);
        let frame = reader.next_frame().unwrap().expect("a whole frame");
        let message = Message::decode(&frame).unwrap();
        let device = c.ends[to];
        let replies = c.sessions[to]
            .receive(&self.devices[device], message, now())
            .unwrap();
        let state = c.sessions[to].state();
        assert!(
            !matches!(state, State::Behind | State::PeerBehind),
            "device {device} is {state:?}"
        );
        for reply in &replies {
            c.pipes[1 - to].send(reply);
        }
        true
    }

    fn deliver_everything(&mut self) {
        for _ in 0..10_000 {
            let mut any = false;
            for c in 0..self.connections.len() {
                for to in 0..2 {
                    any |= self.deliver(c, to);
                }
            }
            for d in 0..DEVICES {
                self.flush(d);
            }
            let in_flight = self
                .connections
                .iter()
                .any(|c| c.pipes.iter().any(|p| !p.frames.is_empty()));
            if !any && !in_flight {
                return;
            }
        }
        panic!("the network never went quiet");
    }
}

#[derive(Debug, Clone)]
enum Step {
    /// Add a block, under an existing one if there are any.
    Edit {
        device: usize,
        parent: Index,
        text: u8,
    },
    /// Replace an existing block's text.
    Rewrite {
        device: usize,
        block: Index,
        text: u8,
    },
    Connect(usize, usize),
    Disconnect(Index),
    Deliver {
        connection: Index,
        to: usize,
    },
    /// Send a device's batched updates.
    Flush(usize),
    /// The device records that it has stored what it has.
    Store(usize),
    /// The device drops history, if its compaction point says it can.
    Compact(usize),
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        3 => (0..DEVICES, any::<Index>(), any::<u8>()).prop_map(|(device, parent, text)| Step::Edit { device, parent, text }),
        1 => (0..DEVICES, any::<Index>(), any::<u8>()).prop_map(|(device, block, text)| Step::Rewrite { device, block, text }),
        1 => (0..DEVICES, 0..DEVICES).prop_map(|(a, b)| Step::Connect(a, b)),
        1 => any::<Index>().prop_map(Step::Disconnect),
        6 => (any::<Index>(), 0..2usize).prop_map(|(connection, to)| Step::Deliver { connection, to }),
        2 => (0..DEVICES).prop_map(Step::Flush),
        1 => (0..DEVICES).prop_map(Step::Store),
        1 => (0..DEVICES).prop_map(Step::Compact),
    ]
}

fn layout(graph: &Graph) -> Vec<(BlockId, Option<BlockId>, String)> {
    graph
        .blocks()
        .into_iter()
        .map(|b| (b, graph.parent(b), graph.text(b).unwrap()))
        .collect()
}

proptest! {
    // 64 cases by default; PROPTEST_CASES overrides.
    #![proptest_config(ProptestConfig {
        cases: std::env::var("PROPTEST_CASES").ok().and_then(|c| c.parse().ok()).unwrap_or(64),
        ..ProptestConfig::default()
    })]

    #[test]
    fn devices_converge_through_relays(steps in vec(step(), 1..80)) {
        let mut net = Network::new();
        for step in &steps {
            if let Step::Edit { device, .. } | Step::Rewrite { device, .. } | Step::Store(device) | Step::Compact(device) = *step
                && blank(&net.devices[device])
            {
                continue;
            }
            match *step {
                Step::Store(device) => net.devices[device].record_stored(&device.to_string(), now()).unwrap(),
                Step::Compact(device) => {
                    let graph = &net.devices[device];
                    if let Some(at) = graph.compaction_point(now(), ACTIVE) {
                        net.devices[device] = Graph::from_snapshot(&graph.shallow_snapshot(&at).unwrap()).unwrap();
                    }
                }
                Step::Edit { device, parent, text } => {
                    let graph = &net.devices[device];
                    let blocks = graph.blocks();
                    let parent = (!blocks.is_empty()).then(|| blocks[parent.index(blocks.len())]);
                    graph.create_block(parent, None, &format!("{text}")).unwrap();
                    graph.commit();
                }
                Step::Rewrite { device, block, text } => {
                    let graph = &net.devices[device];
                    let blocks = graph.blocks();
                    if blocks.is_empty() {
                        continue;
                    }
                    graph.set_text(blocks[block.index(blocks.len())], &format!("re{text}")).unwrap();
                    graph.commit();
                }
                Step::Flush(device) => net.flush(device),
                Step::Connect(a, b) => net.connect(a, b),
                Step::Disconnect(i) => {
                    if !net.connections.is_empty() {
                        net.connections.remove(i.index(net.connections.len()));
                    }
                }
                Step::Deliver { connection, to } => {
                    if !net.connections.is_empty() {
                        let c = connection.index(net.connections.len());
                        net.deliver(c, to);
                    }
                }
            }
        }

        // First settle on the connections as they are. Long-lived sessions
        // must have kept track of what their peers have: every group of
        // connected devices agrees, without any new hellos.
        net.deliver_everything();
        let mut group: Vec<usize> = (0..DEVICES).collect();
        fn root(group: &mut [usize], mut d: usize) -> usize {
            while group[d] != d {
                d = group[d];
            }
            d
        }
        for c in &net.connections {
            let (a, b) = (root(&mut group, c.ends[0]), root(&mut group, c.ends[1]));
            group[a] = b;
        }
        for d in 0..DEVICES {
            let r = root(&mut group, d);
            prop_assert_eq!(layout(&net.devices[d]), layout(&net.devices[r]), "devices {} and {} are connected", d, r);
        }

        // Then reconnect them in a chain, 0 - 1 - 2, so that everything has
        // to be relayed through the middle.
        net.connections.clear();
        for d in 1..DEVICES {
            net.connect(d - 1, d);
        }
        net.deliver_everything();

        let expected = layout(&net.devices[0]);
        for device in &net.devices {
            prop_assert_eq!(&layout(device), &expected);
        }
        // Every session knows its peer is up to date.
        for c in &mut net.connections {
            for side in 0..2 {
                prop_assert_eq!(c.sessions[side].state(), &State::Syncing);
                prop_assert!(c.sessions[side].flush(&net.devices[c.ends[side]]).unwrap().is_none());
            }
        }
    }
}
