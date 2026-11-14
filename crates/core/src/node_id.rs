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

use crate::{Error, Graph, NodeId, Result};

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
            .map_err(|_| Error::InvalidNodeId)
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
}
