//! Saving a graph shortly after it changes.

use std::collections::HashSet;
use std::time::Duration;

use quiddity_node::keyring::Keyring;
use quiddity_node::store::Store;
use quiddity_node::{Allowed, Peer, save_on_change};

#[tokio::test]
async fn a_change_is_saved_until_the_task_is_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path(), Keyring::new(dir.path()));
    let graph = store.init(false).unwrap();
    let peer = Peer::new(graph, Allowed::Only(HashSet::new()), "device");
    let saver = save_on_change(peer.clone(), store.clone());

    peer.graph().create_block(None, None, "Saved").unwrap();
    peer.graph().commit();
    peer.changed();

    let saved = async {
        while !store
            .load()
            .unwrap()
            .version()
            .includes(&peer.graph().version())
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), saved)
        .await
        .expect("the change was never saved");

    // Stopping it lets go of the peer, which the app relies on when it
    // switches graphs: nothing else is left holding the graph's files.
    saver.abort();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !saver.is_finished() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the task was not stopped");
}
