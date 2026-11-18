//! Presence over a simulated network: devices connect, drop links, crash,
//! restart and move around, and announcements travel between neighbours in
//! random order. Once they have settled, every device must know exactly the
//! devices it can reach, and where they are looking.
//!
//! A crashed device says nothing: only the devices beside it notice.

use std::collections::{BTreeMap, BTreeSet};

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::Index;
use quiddity_sync::{Presence, Record};

const DEVICES: usize = 5;
const BLOCKS: u8 = 3;

fn name(i: usize) -> String {
    format!("d{i}")
}

#[derive(Debug, Clone)]
enum Step {
    Connect(Index, Index),
    Drop(Index, Index),
    Crash(Index),
    Restart(Index),
    Focus(Index, Vec<u8>),
}

fn step() -> impl Strategy<Value = Step> {
    let device = || any::<Index>();
    prop_oneof![
        3 => (device(), device()).prop_map(|(a, b)| Step::Connect(a, b)),
        2 => (device(), device()).prop_map(|(a, b)| Step::Drop(a, b)),
        1 => device().prop_map(Step::Crash),
        1 => device().prop_map(Step::Restart),
        3 => (device(), vec(0..BLOCKS, 0..3)).prop_map(|(d, b)| Step::Focus(d, b)),
    ]
}

struct Network {
    nodes: Vec<Option<Presence>>,
    links: BTreeSet<(usize, usize)>,
    epoch: u64,
    /// Announcements in flight: (to, from, record).
    pending: Vec<(usize, usize, Record)>,
    /// Picks which announcement arrives next.
    shuffle: u64,
}

impl Network {
    fn new(shuffle: u64) -> Self {
        let mut net = Self {
            nodes: Vec::new(),
            links: BTreeSet::new(),
            epoch: 0,
            pending: Vec::new(),
            shuffle,
        };
        for i in 0..DEVICES {
            let node = net.start(i);
            net.nodes.push(Some(node));
        }
        net
    }

    fn start(&mut self, i: usize) -> Presence {
        self.epoch += 1;
        Presence::new(name(i), self.epoch)
    }

    fn node(&mut self, i: usize) -> &mut Presence {
        self.nodes[i].as_mut().unwrap()
    }

    fn neighbours(&self, i: usize) -> Vec<usize> {
        (0..DEVICES)
            .filter(|&j| self.links.contains(&(i.min(j), i.max(j))))
            .collect()
    }

    fn apply(&mut self, step: &Step) {
        match step {
            Step::Connect(a, b) => {
                let (a, b) = (a.index(DEVICES), b.index(DEVICES));
                if a == b || self.nodes[a].is_none() || self.nodes[b].is_none() {
                    return;
                }
                if self.links.insert((a.min(b), a.max(b))) {
                    self.node(a).connect(&name(b));
                    self.node(b).connect(&name(a));
                }
            }
            Step::Drop(a, b) => {
                let (a, b) = (a.index(DEVICES), b.index(DEVICES));
                if self.links.remove(&(a.min(b), a.max(b))) {
                    // Whatever was in flight between them is lost.
                    self.pending
                        .retain(|&(to, from, _)| (to, from) != (a, b) && (to, from) != (b, a));
                    self.node(a).disconnect(&name(b));
                    self.node(b).disconnect(&name(a));
                }
            }
            Step::Crash(a) => {
                let a = a.index(DEVICES);
                let neighbours = self.neighbours(a);
                self.nodes[a] = None;
                self.links.retain(|&(x, y)| x != a && y != a);
                self.pending.retain(|&(to, from, _)| to != a && from != a);
                for n in neighbours {
                    self.node(n).disconnect(&name(a));
                }
            }
            Step::Restart(a) => {
                let a = a.index(DEVICES);
                if self.nodes[a].is_none() {
                    self.nodes[a] = Some(self.start(a));
                }
            }
            Step::Focus(a, blocks) => {
                let a = a.index(DEVICES);
                let focus = blocks.iter().map(ToString::to_string).collect();
                if let Some(node) = self.nodes[a].as_mut() {
                    node.set_focus(focus);
                }
            }
        }
    }

    /// Lets devices announce and deliveries happen, in a scrambled order,
    /// until nothing is left to say.
    fn settle(&mut self) {
        loop {
            for i in 0..DEVICES {
                let Some(node) = self.nodes[i].as_mut() else {
                    continue;
                };
                for to in (0..DEVICES).filter(|&j| self.links.contains(&(i.min(j), i.max(j)))) {
                    for record in node.outgoing(&name(to)) {
                        self.pending.push((to, i, record));
                    }
                }
            }
            if self.pending.is_empty() {
                return;
            }
            // Deliver some, then let everyone speak again.
            for _ in 0..=self.pending.len() / 2 {
                self.shuffle = self
                    .shuffle
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let at = (self.shuffle >> 33) as usize % self.pending.len();
                let (to, from, record) = self.pending.swap_remove(at);
                self.node(to).receive(&name(from), record);
            }
        }
    }

    /// The devices `i` can reach, `i` included.
    fn component(&self, i: usize) -> BTreeSet<usize> {
        let mut seen = BTreeSet::from([i]);
        let mut frontier = vec![i];
        while let Some(d) = frontier.pop() {
            for n in self.neighbours(d) {
                if seen.insert(n) {
                    frontier.push(n);
                }
            }
        }
        seen
    }

    fn check(&self) -> Result<(), TestCaseError> {
        for i in 0..DEVICES {
            let Some(node) = &self.nodes[i] else { continue };
            let reachable = self.component(i);
            let live: BTreeSet<String> = reachable.iter().copied().map(name).collect();
            prop_assert_eq!(node.live(), live, "what {} sees", name(i));
            let present: BTreeSet<_> = node.present().iter().map(|r| r.device.clone()).collect();
            let others: BTreeSet<_> = reachable
                .iter()
                .filter(|&&j| j != i)
                .copied()
                .map(name)
                .collect();
            prop_assert_eq!(present, others, "who {} sees besides itself", name(i));

            let mut watched = BTreeMap::<String, bool>::new();
            for &j in &reachable {
                let theirs = self.nodes[j].as_ref().unwrap().own();
                prop_assert_eq!(node.record(&name(j)), Some(theirs));
                for block in &theirs.focus {
                    *watched.entry(block.clone()).or_default() |= j != i;
                }
            }
            for block in 0..BLOCKS {
                let block = block.to_string();
                let expected = watched.get(&block).copied().unwrap_or(false);
                prop_assert_eq!(
                    node.watched(&block),
                    expected,
                    "{} watching {}",
                    name(i),
                    block
                );
            }
        }
        Ok(())
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn devices_know_who_they_can_reach_and_where_they_are(
        steps in vec(step(), 1..40),
        shuffle in any::<u64>(),
    ) {
        let mut net = Network::new(shuffle);
        for step in &steps {
            net.apply(step);
            net.settle();
            net.check()?;
        }
    }
}
