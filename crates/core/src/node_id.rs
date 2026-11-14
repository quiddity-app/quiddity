//! Which replica this is.
//!
//! A replica's node ID is the peer ID Loro stamps on each edit it makes. Loro
//! chooses a random one for every replica it starts, which is right for a
//! replica that lasts as long as the process but wrong for one that is kept: a
//! graph that is opened a hundred times would hold a hundred peers in its
//! version for ever. So a replica that is kept is given its own, which it
//! keeps.
//!
//! Two replicas must never edit as the same node: Loro can't hold two
//! histories under one ID, and says nothing. That is why a node ID is chosen
//! at random, never derived, and why whoever keeps one (see `quiddity-node`'s
//! store) ties it to a device and a place.

use loro::PeerID;
use tracing::warn;

use crate::{Error, Graph, NodeId, Result, Version};

impl Graph {
    /// This replica's node ID: the peer ID of the edits it makes.
    pub fn node_id(&self) -> NodeId {
        NodeId::new(self.doc().peer_id())
    }

    /// Makes this replica edit as `node` from now on. What it wrote before is
    /// left as it was, under the ID it had then.
    ///
    /// If `node` has edits in this graph already, the next is the one after
    /// them: this is how a replica that is kept carries on as itself. So it
    /// must be a node that no other replica is editing as, or the two will
    /// make different edits with the same ID, and Loro will keep one of them
    /// without saying.
    pub fn set_node_id(&self, node: NodeId) -> Result<()> {
        self.doc()
            .set_peer_id(node.get())
            .map_err(|_| Error::InvalidNodeId)?;
        // What it holds under this ID now is not its own work: only what it
        // makes from here on is.
        let held = self.version().count_of(node);
        self.nodes
            .lock()
            .expect("nodes lock poisoned")
            .entry(node.get())
            .or_insert(held);
        Ok(())
    }

    /// The node IDs this replica gave up, because changes arrived under them that
    /// it could not take (see [`Error::Twin`]), oldest first. Whoever keeps the
    /// replica should keep [`Graph::node_id`] instead, which is a new one.
    pub fn retired_node_ids(&self) -> Vec<NodeId> {
        self.retired.lock().expect("retired lock poisoned").clone()
    }

    /// Refuses changes that would clash with this replica's own edits, and gives
    /// up the node ID they clash under if it is the one in use.
    ///
    /// A replica has edited as a node when it holds more under that ID than it
    /// did when it took it. Changes that carry more than it holds under the same
    /// ID are then another replica's: either a twin made by copying, or this
    /// replica's own earlier history, restored from behind and then edited. They
    /// are not told apart, since the counts are all there is to compare, and
    /// both are refused. A replica that has made nothing of its own under the ID
    /// just takes its lost edits back, as when it crashed after sending them.
    ///
    /// What this can't see: another replica that holds as much or more under the
    /// ID. It finds *this* one, when it is behind, and the one that is behind is
    /// the one that sees the other's changes carry more. With equal counts and
    /// different edits neither is behind, and Loro silently keeps both sets of
    /// changes apart (`twins_with_equal_counts_cannot_be_told_apart`).
    pub(crate) fn refuse_twins(&self, theirs: &Version) -> Result<()> {
        let ours = self.version();
        let clash = {
            let nodes = self.nodes.lock().expect("nodes lock poisoned");
            nodes.iter().find_map(|(&id, &held)| {
                let node = NodeId::new(id);
                let made = ours.count_of(node);
                (made > held && theirs.count_of(node) > made).then_some(node)
            })
        };
        let Some(node) = clash else {
            return Ok(());
        };
        warn!(%node, "another replica edits as this node: refusing its changes");
        if node == self.node_id() {
            // Edits from here on must not add to the clash.
            self.set_node_id(Self::fresh_node_id())?;
        }
        self.retired
            .lock()
            .expect("retired lock poisoned")
            .push(node);
        Err(Error::Twin(node))
    }

    /// A node ID at random, and never the one Loro refuses (the remainder
    /// leaves out only that one, and the bias of leaving out one in 2^64 is
    /// nothing).
    pub fn fresh_node_id() -> NodeId {
        let bytes = *uuid::Uuid::new_v4().as_bytes();
        let random = u64::from_be_bytes(bytes[..8].try_into().expect("eight bytes of sixteen"));
        NodeId::new(random % PeerID::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_replica_is_given_a_node_id_and_edits_as_it() -> Result<()> {
        let graph = Graph::blank();
        assert_eq!(graph.version().nodes(), []);
        let node = NodeId::new(42);
        graph.set_node_id(node)?;
        assert_eq!(graph.node_id(), node);
        graph.create_block(None, None, "x")?;
        assert_eq!(graph.version().nodes(), [node]);
        assert!(graph.version().count_of(node) > 0);
        assert_eq!(graph.version().count_of(NodeId::new(7)), 0);
        Ok(())
    }

    /// A graph made as a replica has nothing by anyone else, not even what it
    /// starts with.
    #[test]
    fn a_new_graph_made_as_a_replica_is_all_by_it() -> Result<()> {
        let node = NodeId::new(7);
        let graph = Graph::new_as(node);
        graph.create_block(None, None, "x")?;
        assert_eq!(graph.version().nodes(), [node]);
        assert_eq!(graph.node_id(), node);
        // Left alone, a new graph has one of its own, made at random.
        assert_ne!(Graph::new().node_id(), Graph::new().node_id());
        Ok(())
    }

    #[test]
    fn changing_node_leaves_what_was_written_under_the_first() -> Result<()> {
        let graph = Graph::new_as(NodeId::new(1));
        graph.create_block(None, None, "by one")?;
        let before = graph.version().count_of(NodeId::new(1));
        graph.set_node_id(NodeId::new(2))?;
        graph.create_block(None, None, "by two")?;
        assert_eq!(graph.version().nodes(), [NodeId::new(1), NodeId::new(2)]);
        assert_eq!(graph.version().count_of(NodeId::new(1)), before);
        Ok(())
    }

    /// The replica that is kept carries on from where it was: its next edit
    /// follows its last, which is what makes restoring it from a copy that is
    /// behind the danger it is.
    #[test]
    fn an_id_that_has_edits_carries_on_after_them() -> Result<()> {
        let node = NodeId::new(9);
        let original = Graph::new_as(node);
        original.create_block(None, None, "one")?;
        original.create_block(None, None, "two")?;
        let so_far = original.version().count_of(node);

        let reloaded = Graph::from_snapshot(&original.snapshot()?)?;
        assert_ne!(
            reloaded.node_id(),
            node,
            "a loaded replica starts as another"
        );
        reloaded.set_node_id(node)?;
        reloaded.create_block(None, None, "three")?;
        assert!(reloaded.version().count_of(node) > so_far);
        assert_eq!(reloaded.version().nodes(), [node]);
        Ok(())
    }

    #[test]
    fn the_one_id_loro_keeps_is_refused() {
        let graph = Graph::blank();
        assert!(matches!(
            graph.set_node_id(NodeId::new(u64::MAX)),
            Err(Error::InvalidNodeId)
        ));
        assert_ne!(graph.node_id(), NodeId::new(u64::MAX));
        assert!(graph.set_node_id(NodeId::new(0)).is_ok());
    }

    #[test]
    fn fresh_ids_differ_and_are_usable() {
        let ids: std::collections::HashSet<_> = (0..64).map(|_| Graph::fresh_node_id()).collect();
        assert_eq!(ids.len(), 64);
        assert!(ids.iter().all(|id| id.get() != u64::MAX));
        let graph = Graph::blank();
        assert!(ids.iter().all(|&id| graph.set_node_id(id).is_ok()));
    }

    /// Two replicas of one graph that are both editing as one node, each having
    /// made `a` and `b` edits since they were one.
    fn twins(a: usize, b: usize) -> (Graph, Graph, NodeId) {
        let node = NodeId::new(5);
        let base = Graph::new_as(node);
        base.create_block(None, None, "base").unwrap();
        base.commit();
        let snapshot = base.snapshot().unwrap();
        let make = |edits: usize, name: &str| {
            let graph = Graph::from_snapshot(&snapshot).unwrap();
            graph.set_node_id(node).unwrap();
            for i in 0..edits {
                graph
                    .create_block(None, None, &format!("{name}{i}"))
                    .unwrap();
            }
            graph.commit();
            graph
        };
        (make(a, "A"), make(b, "B"), node)
    }

    fn texts(graph: &Graph) -> Vec<String> {
        let mut texts: Vec<_> = graph
            .roots()
            .into_iter()
            .map(|r| graph.text(r).unwrap())
            .collect();
        texts.sort();
        texts
    }

    fn give(from: &Graph, to: &Graph) -> Result<()> {
        to.import(&from.updates_since(&to.version())?)
    }

    /// The replica that is behind, with edits of its own under the node, is
    /// refused the other's, and nothing is applied.
    #[test]
    fn the_replica_that_is_behind_refuses_a_twins_changes() -> Result<()> {
        let (a, b, node) = twins(1, 2);
        let before = texts(&a);
        let version = a.version();
        assert!(matches!(give(&b, &a), Err(Error::Twin(n)) if n == node));
        assert_eq!(texts(&a), before, "nothing was applied");
        assert_eq!(a.version(), version);
        // It stopped editing as the node, so what it does next adds nothing to the clash.
        assert_ne!(a.node_id(), node);
        assert_eq!(a.retired_node_ids(), [node]);
        a.create_block(None, None, "after")?;
        assert_eq!(a.version().count_of(node), version.count_of(node));
        // It is refused again, for the clash is still there.
        assert!(matches!(give(&b, &a), Err(Error::Twin(_))));
        Ok(())
    }

    /// And the one that is ahead cannot tell: the other's changes are a prefix of
    /// what it has, or look so. It is the behind one that finds it.
    #[test]
    fn the_replica_that_is_ahead_cannot_tell() -> Result<()> {
        let (a, b, node) = twins(1, 2);
        give(&a, &b)?;
        assert_eq!(b.node_id(), node);
        assert!(b.retired_node_ids().is_empty());
        Ok(())
    }

    /// What Loro does with two histories under one ID, which is why refusing is
    /// the only safe thing: both imports succeed, each replica's version says it
    /// has the other's, and what they hold is not the same. If this starts to
    /// fail, Loro has changed, and the rest of this module wants another look.
    #[test]
    fn twins_with_equal_counts_cannot_be_told_apart() -> Result<()> {
        let (a, b, node) = twins(2, 2);
        let (va, vb) = (a.version(), b.version());
        give(&b, &a)?;
        give(&a, &b)?;
        assert!(a.retired_node_ids().is_empty() && b.retired_node_ids().is_empty());
        assert_eq!(a.node_id(), node);
        assert!(a.version().includes(&vb) && b.version().includes(&va));
        assert_ne!(
            texts(&a),
            texts(&b),
            "each keeps its own, and says it has both"
        );
        Ok(())
    }

    /// The same, when one is ahead by some: the one that applies the other's later
    /// changes takes some and drops others. This is what refusing avoids.
    #[test]
    fn unrefused_twins_would_have_mixed_their_edits() -> Result<()> {
        let (a, b, _) = twins(1, 2);
        // Applied below the guard, as Loro itself would.
        a.doc().import(&b.updates_since(&a.version())?)?;
        assert_eq!(
            texts(&a),
            ["A0", "B1", "base"],
            "B0 was dropped and B1 taken"
        );
        Ok(())
    }

    /// A replica that has made nothing of its own under the node takes back what
    /// it lost: it crashed after sending edits that it hadn't yet saved, to a
    /// device that kept them. Its next edits follow them, and that device, which
    /// never edited as the node, takes those in turn.
    #[test]
    fn a_replica_that_lost_its_edits_takes_them_back() -> Result<()> {
        let node = NodeId::new(9);
        let before_crash = Graph::new_as(node);
        before_crash.create_block(None, None, "one")?;
        before_crash.commit();
        let backup = before_crash.snapshot()?;
        let peer = Graph::from_snapshot(&backup)?;
        peer.set_node_id(NodeId::new(77))?;
        before_crash.create_block(None, None, "two")?;
        before_crash.create_block(None, None, "three")?;
        before_crash.commit();
        give(&before_crash, &peer)?;
        drop(before_crash);

        let restored = Graph::from_snapshot(&backup)?;
        restored.set_node_id(node)?;
        give(&peer, &restored)?;
        assert_eq!(restored.node_id(), node, "it is still that node");
        assert!(restored.retired_node_ids().is_empty());
        assert_eq!(texts(&restored), texts(&peer));
        restored.create_block(None, None, "four")?;
        restored.commit();
        give(&restored, &peer)?;
        assert_eq!(texts(&peer), texts(&restored));
        assert_eq!(texts(&peer).len(), 4);
        assert!(peer.retired_node_ids().is_empty());
        Ok(())
    }

    /// If it edited before it took them back, the clash is real and it is refused.
    #[test]
    fn a_replica_that_edited_before_taking_them_back_is_refused() -> Result<()> {
        let node = NodeId::new(9);
        let original = Graph::new_as(node);
        original.create_block(None, None, "one")?;
        original.commit();
        let backup = original.snapshot()?;
        for text in ["two", "three", "four"] {
            original.create_block(None, None, text)?;
        }
        original.commit();

        let restored = Graph::from_snapshot(&backup)?;
        restored.set_node_id(node)?;
        restored.create_block(None, None, "only here")?;
        restored.commit();
        let before = texts(&restored);
        assert!(matches!(give(&original, &restored), Err(Error::Twin(n)) if n == node));
        assert_eq!(texts(&restored), before);
        Ok(())
    }

    /// A replica is sent back what it made itself: that is not a clash.
    #[test]
    fn being_sent_ones_own_edits_back_is_not_a_clash() -> Result<()> {
        let (a, b, node) = twins(0, 0);
        a.create_block(None, None, "from a")?;
        a.commit();
        b.set_node_id(NodeId::new(6))?;
        give(&a, &b)?;
        give(&b, &a)?;
        let c = Graph::from_snapshot(&b.snapshot()?)?;
        give(&c, &a)?;
        assert_eq!(a.node_id(), node);
        assert!(a.retired_node_ids().is_empty());
        Ok(())
    }

    /// A node ID this replica never edited as is never a clash, whatever
    /// arrives under it.
    #[test]
    fn a_node_that_was_not_edited_as_is_nothing_to_clash_with() -> Result<()> {
        let (a, _, node) = twins(0, 3);
        // `a` took the node and made nothing under it.
        let (_, b, _) = twins(0, 3);
        give(&b, &a)?;
        assert_eq!(a.node_id(), node);
        Ok(())
    }

    use proptest::prelude::*;

    #[derive(Debug, Clone)]
    enum Step {
        Edit(usize),
        Sync(usize, usize),
    }

    fn steps() -> impl Strategy<Value = Vec<Step>> {
        proptest::collection::vec(
            prop_oneof![
                (0..3usize).prop_map(Step::Edit),
                (0..3usize, 0..3usize).prop_map(|(a, b)| Step::Sync(a, b)),
            ],
            0..40,
        )
    }

    proptest! {
        /// Replicas that each edit as a node of their own are never refused,
        /// however edits and syncs interleave and however edits are relayed back
        /// to where they came from, and they all end up the same.
        #[test]
        fn replicas_that_edit_as_nodes_of_their_own_are_never_refused(steps in steps()) {
            let base = Graph::new_as(NodeId::new(1));
            base.create_block(None, None, "base").unwrap();
            base.commit();
            let snapshot = base.snapshot().unwrap();
            let replicas: Vec<_> = (0..3u64)
                .map(|i| {
                    let graph = Graph::from_snapshot(&snapshot).unwrap();
                    graph.set_node_id(NodeId::new(100 + i)).unwrap();
                    graph
                })
                .collect();
            for (n, step) in steps.iter().enumerate() {
                match *step {
                    Step::Edit(i) => {
                        replicas[i].create_block(None, None, &format!("edit {n}")).unwrap();
                        replicas[i].commit();
                    }
                    Step::Sync(from, to) if from != to => {
                        prop_assert!(give(&replicas[from], &replicas[to]).is_ok());
                    }
                    Step::Sync(..) => {}
                }
            }
            for _ in 0..2 {
                for from in 0..3 {
                    for to in 0..3 {
                        if from != to {
                            prop_assert!(give(&replicas[from], &replicas[to]).is_ok());
                        }
                    }
                }
            }
            prop_assert!(replicas.iter().all(|r| r.retired_node_ids().is_empty()));
            prop_assert!(replicas.iter().all(|r| texts(r) == texts(&replicas[0])));
        }
    }
}
