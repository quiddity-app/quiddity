//! What a session has running, so that switching graphs can stop it all.

use std::sync::{Arc, Mutex, MutexGuard};

use iroh::Endpoint;
use tauri::async_runtime::JoinHandle;
use tokio::sync::oneshot;

/// What a session has running: the UI's subscriptions hold the session, and
/// the network holds the graph's files and its endpoint, so a graph opened
/// again later would find them taken. Cloning shares it, which is how the
/// process can hold it beyond the window that started it (`process.rs`).
#[derive(Clone, Default)]
pub struct Running(Arc<Mutex<Tasks>>);

#[derive(Default)]
pub struct Tasks {
    /// Tells the network task to close its endpoint.
    pub stop: Option<oneshot::Sender<()>>,
    pub network: Option<JoinHandle<()>>,
    pub others: Vec<JoinHandle<()>>,
    /// The network's endpoint while it is up, for `network_changed`. Shared
    /// with the network task, which fills it and empties it, so that a graph
    /// switched away from leaves nothing behind to be told.
    pub endpoint: Arc<Mutex<Option<Endpoint>>>,
}

impl Running {
    pub fn tasks(&self) -> MutexGuard<'_, Tasks> {
        self.0.lock().expect("running lock poisoned")
    }

    /// Stops the session's tasks, and waits for the network to close.
    pub async fn stop(&self) {
        let tasks = std::mem::take(&mut *self.tasks());
        for task in &tasks.others {
            task.abort();
        }
        if let Some(stop) = tasks.stop {
            // A network that has already ended has nothing to stop.
            let _ = stop.send(());
        }
        if let Some(network) = tasks.network {
            drop(network.await);
        }
    }
}
