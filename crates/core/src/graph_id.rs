//! Which graph this is.
//!
//! The ID is made with the graph, in its first edit, and lives in the graph's
//! own map in the document beside the schema version, so every replica has
//! it. A blank replica has none until it imports the graph, so two devices
//! never each invent one and then have to agree.
//!
//! A graph made before this existed has none, and keeps working without: it
//! is given one only when asked ([`Graph::assign_graph_id`]), on one device.
//! Two replicas of one graph that each made their own would never agree.

use loro::{LoroValue, ValueOrContainer};
use quiddity_identity::GraphId;

use crate::graph::GRAPH;
use crate::{Error, Graph, Result};

const GRAPH_ID: &str = "graphId";

impl Graph {
    /// This graph's ID, if it has one this build can read. A blank replica
    /// has none yet, nor has a graph made before graphs had them, nor one
    /// whose ID was written in a form this build doesn't know.
    pub fn graph_id(&self) -> Option<GraphId> {
        match self.doc().get_map(GRAPH).get(GRAPH_ID) {
            Some(ValueOrContainer::Value(LoroValue::String(text))) => text.parse().ok(),
            _ => None,
        }
    }

    /// Gives a graph that has no ID one, and returns it. The ID is an edit and
    /// syncs like any other.
    ///
    /// Do this on one device only, and let it sync: another that does the
    /// same before it arrives makes a second ID, one of the two wins when they
    /// meet, and a replica holding the loser's is refused by every peer that
    /// holds the winner's. That is why nothing does it by itself.
    ///
    /// Refused if there is an ID, or something where one should be that this
    /// build can't read (which a newer build wrote, and which this must not
    /// replace), and on a blank replica, which can't edit before it has joined.
    pub fn assign_graph_id(&self) -> Result<GraphId> {
        if self.schema_version().is_none() {
            return Err(Error::BlankReplica);
        }
        if self.doc().get_map(GRAPH).get(GRAPH_ID).is_some() {
            return Err(Error::GraphIdExists);
        }
        let id = Self::fresh_graph_id();
        self.write_graph_id(id);
        self.commit();
        Ok(id)
    }

    pub(crate) fn fresh_graph_id() -> GraphId {
        // A version 4 UUID: random, apart from the six bits that say so.
        GraphId::from_bytes(*uuid::Uuid::new_v4().as_bytes())
    }

    pub(crate) fn write_graph_id(&self, id: GraphId) {
        self.doc()
            .get_map(GRAPH)
            .insert(GRAPH_ID, id.to_string())
            .expect("inserting a string into a map can't fail");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// A graph from before graphs had IDs.
    fn legacy() -> Graph {
        let graph = Graph::new();
        graph.doc().get_map(GRAPH).delete(GRAPH_ID).unwrap();
        graph.commit();
        assert_eq!(graph.graph_id(), None);
        graph
    }

    #[test]
    fn a_new_graph_has_an_id_of_its_own() {
        let (a, b) = (Graph::new(), Graph::new());
        assert!(a.graph_id().is_some());
        assert_ne!(a.graph_id(), b.graph_id());
        assert_eq!(a.graph_id(), a.graph_id());
    }

    #[test]
    fn a_blank_replica_has_none_until_it_joins() -> Result<()> {
        let graph = Graph::new();
        graph.create_block(None, None, "x")?;
        assert_eq!(Graph::blank().graph_id(), None);

        let by_snapshot = Graph::from_snapshot(&graph.snapshot()?)?;
        assert_eq!(by_snapshot.graph_id(), graph.graph_id());

        let blank = Graph::blank();
        blank.import(&graph.updates_since(&blank.version())?)?;
        assert_eq!(blank.graph_id(), graph.graph_id());
        Ok(())
    }

    #[test]
    fn it_survives_compaction() -> Result<()> {
        let graph = Graph::new();
        graph.create_block(None, None, "x")?;
        let compacted = Graph::from_snapshot(&graph.shallow_snapshot(&graph.version())?)?;
        assert_eq!(compacted.graph_id(), graph.graph_id());
        Ok(())
    }

    #[test]
    fn a_graph_without_one_is_given_one_when_asked() -> Result<()> {
        let graph = legacy();
        let id = graph.assign_graph_id()?;
        assert_eq!(graph.graph_id(), Some(id));
        assert!(matches!(graph.assign_graph_id(), Err(Error::GraphIdExists)));
        assert_eq!(graph.graph_id(), Some(id), "it is not replaced");
        // And it reaches another replica like any edit.
        let other = Graph::from_snapshot(&graph.snapshot()?)?;
        assert_eq!(other.graph_id(), Some(id));
        Ok(())
    }

    #[test]
    fn a_graph_with_one_is_not_given_another() {
        let graph = Graph::new();
        let id = graph.graph_id();
        assert!(matches!(graph.assign_graph_id(), Err(Error::GraphIdExists)));
        assert_eq!(graph.graph_id(), id);
    }

    #[test]
    fn a_blank_replica_is_not_given_one() {
        let blank = Graph::blank();
        assert!(matches!(blank.assign_graph_id(), Err(Error::BlankReplica)));
        assert_eq!(
            blank.version(),
            crate::Version::default(),
            "it made no edit"
        );
    }

    /// What a newer build wrote is left alone, not mistaken for no ID.
    #[test]
    fn an_id_this_build_cannot_read_is_not_replaced() {
        let graph = legacy();
        graph
            .doc()
            .get_map(GRAPH)
            .insert(GRAPH_ID, "qgj_somethingnew")
            .unwrap();
        assert_eq!(graph.graph_id(), None);
        assert!(matches!(graph.assign_graph_id(), Err(Error::GraphIdExists)));
        let stored = graph.doc().get_map(GRAPH).get(GRAPH_ID);
        assert!(stored.is_some());
    }

    /// Two replicas that each gave one graph an ID before meeting agree
    /// after: one wins, for both. (Which is why it is asked of one device.)
    #[test]
    fn replicas_that_both_assigned_one_agree_afterwards() -> Result<()> {
        let one = legacy();
        let two = Graph::from_snapshot(&one.snapshot()?)?;
        two.doc().set_peer_id(7)?;
        let (a, b) = (one.assign_graph_id()?, two.assign_graph_id()?);
        assert_ne!(a, b);
        one.import(&two.updates_since(&one.version())?)?;
        two.import(&one.updates_since(&two.version())?)?;
        assert_eq!(one.graph_id(), two.graph_id());
        assert!(one.graph_id() == Some(a) || one.graph_id() == Some(b));
        Ok(())
    }

    proptest! {
        /// Whatever is stored, reading it never panics, and reads only what
        /// is written as an ID's own text.
        #[test]
        fn only_a_graph_ids_own_text_reads_as_one(text in "(qgi_)?[0-9A-Za-z_]{0,40}") {
            let graph = legacy();
            graph.doc().get_map(GRAPH).insert(GRAPH_ID, text.as_str()).unwrap();
            if let Some(id) = graph.graph_id() {
                prop_assert_eq!(id.to_string(), text);
            }
        }

        #[test]
        fn a_stored_id_reads_back(bytes in any::<[u8; 16]>()) {
            let graph = legacy();
            graph.write_graph_id(GraphId::from_bytes(bytes));
            prop_assert_eq!(graph.graph_id(), Some(GraphId::from_bytes(bytes)));
        }
    }
}
