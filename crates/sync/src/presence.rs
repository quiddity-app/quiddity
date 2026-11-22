use std::collections::{BTreeMap, BTreeSet};

/// What one device says about itself: which blocks it has open, and which
/// devices it holds a connection to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub device: String,
    /// Chosen afresh each time the device starts, so that its records
    /// outrank those of an earlier run, whose sequence numbers it has
    /// forgotten.
    pub epoch: u64,
    /// Increases with every change to the record.
    pub seq: u64,
    /// The devices this one is directly connected to.
    pub peers: BTreeSet<String>,
    /// The blocks this device is viewing or editing.
    pub focus: BTreeSet<String>,
}

impl Record {
    fn stamp(&self) -> (u64, u64) {
        (self.epoch, self.seq)
    }
}

/// Who else is here, and where they are looking: ephemeral state, kept apart
/// from the graph so that it leaves no history.
///
/// Every device has a [`Record`] of its own, and gossips it: to each device
/// it is connected to, it sends the records of every live device that peer
/// hasn't yet been told about, as [`Presence::outgoing`] works out, so
/// records reach the whole connected network. A device is *live* if there is
/// a chain of connections to it that both ends of each link vouch for. A link
/// that one end has dropped therefore cuts off whatever hung from it at once,
/// even if the far side never says it has gone, with no need for timeouts:
/// liveness is whatever the transport says about our own connections,
/// relayed by the devices beside us.
///
/// Records of devices that are no longer live are kept, one per device. That
/// costs little, and they are sent again if the device comes back into reach.
#[derive(Debug)]
pub struct Presence {
    me: String,
    records: BTreeMap<String, Record>,
    /// For each connected peer, the stamp of the newest record of each device
    /// it is known to have: what it said, plus what we have sent.
    told: BTreeMap<String, BTreeMap<String, (u64, u64)>>,
}

impl Presence {
    /// Presence for device `me`, which has just started, with a unique
    /// `epoch` greater than that of any earlier run.
    pub fn new(me: impl Into<String>, epoch: u64) -> Self {
        let me = me.into();
        let record = Record {
            device: me.clone(),
            epoch,
            seq: 0,
            peers: BTreeSet::new(),
            focus: BTreeSet::new(),
        };
        Self {
            records: BTreeMap::from([(me.clone(), record)]),
            told: BTreeMap::new(),
            me,
        }
    }

    /// This device's own record.
    pub fn own(&self) -> &Record {
        &self.records[&self.me]
    }

    /// The latest record held for `device`, live or not.
    pub fn record(&self, device: &str) -> Option<&Record> {
        self.records.get(device)
    }

    /// Sets the blocks this device has open.
    pub fn set_focus(&mut self, focus: BTreeSet<String>) {
        self.update(|r| {
            let changed = r.focus != focus;
            r.focus = focus;
            changed
        });
    }

    /// A connection to `peer` is up. It knows nothing yet, so everything
    /// live is sent to it by [`Presence::outgoing`].
    pub fn connect(&mut self, peer: &str) {
        self.told.insert(peer.to_owned(), BTreeMap::new());
        self.update(|r| r.peers.insert(peer.to_owned()));
    }

    /// The connection to `peer` is gone, whether it was closed or lost. The
    /// transport is the one to say.
    pub fn disconnect(&mut self, peer: &str) {
        self.told.remove(peer);
        self.update(|r| r.peers.remove(peer));
    }

    fn update(&mut self, change: impl FnOnce(&mut Record) -> bool) {
        let own = self.records.get_mut(&self.me).expect("own record is kept");
        if change(own) {
            own.seq += 1;
        }
    }

    /// Takes a record that `from`, a connected peer, has sent.
    pub fn receive(&mut self, from: &str, record: Record) {
        // Our own record is ours to write, whatever is said about it.
        if record.device == self.me {
            return;
        }
        // The peer has it, and needn't be sent it back.
        if let Some(told) = self.told.get_mut(from) {
            let stamp = told.entry(record.device.clone()).or_default();
            *stamp = (*stamp).max(record.stamp());
        }
        let newer = self
            .records
            .get(&record.device)
            .is_none_or(|held| record.stamp() > held.stamp());
        if newer {
            self.records.insert(record.device.clone(), record);
        }
    }

    /// What to send to `peer`: the records of live devices that it hasn't
    /// been told of. Call after anything changes. Nothing for a device that
    /// isn't connected.
    pub fn outgoing(&mut self, peer: &str) -> Vec<Record> {
        let live = self.live();
        let Some(told) = self.told.get_mut(peer) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for record in self.records.values().filter(|r| live.contains(&r.device)) {
            // A peer knows its own record best.
            let stale = told.get(&record.device).is_none_or(|&t| record.stamp() > t);
            if stale && record.device != peer {
                told.insert(record.device.clone(), record.stamp());
                out.push(record.clone());
            }
        }
        out
    }

    /// The records of the other live devices.
    pub fn present(&self) -> Vec<&Record> {
        let live = self.live();
        self.records
            .values()
            .filter(|r| r.device != self.me && live.contains(&r.device))
            .collect()
    }

    /// The live devices, this one included: those reachable over links that
    /// both ends list.
    pub fn live(&self) -> BTreeSet<String> {
        let mut live = BTreeSet::from([self.me.clone()]);
        let mut frontier = vec![&self.me];
        while let Some(device) = frontier.pop() {
            for peer in &self.records[device].peers {
                let agreed = self
                    .records
                    .get(peer)
                    .is_some_and(|r| r.peers.contains(device));
                if agreed && live.insert(peer.clone()) {
                    frontier.push(&self.records[peer].device);
                }
            }
        }
        live
    }

    /// Whether some other live device has `block` open.
    pub fn watched(&self, block: &str) -> bool {
        self.present().iter().any(|r| r.focus.contains(block))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The network as a whole is tested in tests/presence.rs. These cover the
    // rules of a single device's view.

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    fn record(device: &str, epoch: u64, seq: u64, peers: &[&str], focus: &[&str]) -> Record {
        Record {
            device: device.to_owned(),
            epoch,
            seq,
            peers: set(peers),
            focus: set(focus),
        }
    }

    /// `a`, connected to `b`.
    fn connected() -> Presence {
        let mut p = Presence::new("a", 1);
        p.connect("b");
        p
    }

    #[test]
    fn changes_bump_the_sequence_number_once() {
        let mut p = Presence::new("a", 1);
        p.set_focus(set(&["x"]));
        p.set_focus(set(&["x"]));
        assert_eq!(p.own().seq, 1);
        p.connect("b");
        p.connect("b");
        p.disconnect("c");
        assert_eq!(p.own().seq, 2);
        p.disconnect("b");
        assert_eq!(p.own().seq, 3);
        assert_eq!(
            (p.own().focus.clone(), p.own().peers.len()),
            (set(&["x"]), 0)
        );
    }

    #[test]
    fn only_newer_records_are_kept() {
        let mut p = connected();
        p.receive("b", record("b", 1, 3, &[], &[]));
        p.receive("b", record("b", 1, 3, &[], &["x"]));
        p.receive("b", record("b", 1, 2, &[], &["y"]));
        assert!(p.record("b").unwrap().focus.is_empty(), "same, then older");
        p.receive("b", record("b", 1, 4, &[], &["x"]));
        assert_eq!(p.record("b").unwrap().focus, set(&["x"]));
        // A restart outranks the last run, whatever its sequence number.
        p.receive("b", record("b", 2, 0, &[], &[]));
        p.receive("b", record("b", 1, 9, &[], &["y"]));
        assert_eq!(p.record("b").unwrap().epoch, 2);
    }

    #[test]
    fn nobody_else_writes_our_record() {
        let mut p = connected();
        p.receive("b", record("a", 9, 9, &["b"], &["x"]));
        assert_eq!(p.own().epoch, 1);
        assert!(p.own().focus.is_empty());
    }

    #[test]
    fn a_link_counts_only_if_both_ends_list_it() {
        let mut p = connected();
        // b hasn't said it's connected to us.
        p.receive("b", record("b", 1, 1, &[], &[]));
        assert_eq!(p.live(), set(&["a"]));
        // b is, and c hangs from b, but c claims a link b doesn't list.
        p.receive("b", record("b", 1, 2, &["a"], &[]));
        p.receive("b", record("c", 1, 1, &["b"], &[]));
        assert_eq!(p.live(), set(&["a", "b"]));
        p.receive("b", record("b", 1, 3, &["a", "c"], &[]));
        assert_eq!(p.live(), set(&["a", "b", "c"]));
    }

    /// If b drops c, c is gone from here without c, or anyone, saying so.
    #[test]
    fn what_hangs_from_a_dropped_link_is_dropped() {
        let mut p = connected();
        p.receive("b", record("b", 1, 1, &["a", "c"], &[]));
        p.receive("b", record("c", 1, 1, &["b", "d"], &["x"]));
        p.receive("b", record("d", 1, 1, &["c"], &["x"]));
        assert_eq!(p.live().len(), 4);
        assert!(p.watched("x"));

        p.receive("b", record("b", 1, 2, &["a"], &[]));
        assert_eq!(p.live(), set(&["a", "b"]));
        assert!(!p.watched("x"));
    }

    #[test]
    fn we_do_not_watch_ourselves() {
        let mut p = Presence::new("a", 1);
        p.set_focus(set(&["x"]));
        assert!(!p.watched("x"));
    }

    /// Records outlive their devices' liveness, so they needn't be sent
    /// again when a device is back in reach.
    #[test]
    fn losing_a_link_keeps_the_records_but_not_the_devices() {
        let mut p = connected();
        p.receive("b", record("b", 1, 1, &["a", "c"], &[]));
        p.receive("b", record("c", 1, 1, &["b"], &["x"]));
        p.disconnect("b");
        assert_eq!(p.live(), set(&["a"]));
        assert!(p.record("c").is_some());
        assert!(!p.watched("x"));
        p.connect("b");
        assert_eq!(p.live(), set(&["a", "b", "c"]));
        assert!(p.watched("x"));
    }

    #[test]
    fn peers_are_told_what_they_lack_once() {
        let mut p = Presence::new("a", 1);
        p.connect("b");
        p.connect("c");
        let seqs = |records: Vec<Record>| {
            records
                .iter()
                .map(|r| (r.device.clone(), r.seq))
                .collect::<Vec<_>>()
        };
        assert_eq!(seqs(p.outgoing("b")), [("a".to_owned(), 2)]);
        assert_eq!(seqs(p.outgoing("c")), [("a".to_owned(), 2)]);
        assert!(p.outgoing("b").is_empty() && p.outgoing("c").is_empty());
        assert!(p.outgoing("stranger").is_empty());

        p.set_focus(set(&["x"]));
        assert_eq!(seqs(p.outgoing("b")), [("a".to_owned(), 3)]);

        // Only live devices are passed on: b says it has c, but c doesn't
        // say it has b, and has nothing to say to us.
        p.receive("b", record("b", 1, 1, &["a", "c"], &[]));
        p.receive("b", record("c", 1, 1, &["b"], &[]));
        p.receive("b", record("d", 1, 1, &[], &[]));
        assert!(p.outgoing("b").is_empty(), "b has all it said");
        let to_c = seqs(p.outgoing("c"));
        assert_eq!(
            to_c,
            [("a".to_owned(), 3), ("b".to_owned(), 1)],
            "c is sent neither its own record nor the stranger's"
        );
    }

    /// A peer that reconnects has forgotten everything.
    #[test]
    fn a_new_connection_is_told_everything_again() {
        let mut p = connected();
        p.outgoing("b");
        p.disconnect("b");
        p.connect("b");
        assert_eq!(p.outgoing("b").len(), 1);
    }
}
