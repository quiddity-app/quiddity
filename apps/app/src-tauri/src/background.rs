//! Syncing once, for a run that has a few minutes and then goes: on Android, a
//! job the system starts, possibly in a process the app isn't running in.
//!
//! It opens the graph if nothing has, puts it on the network, waits for the
//! devices it can reach to have what it has and the other way round
//! ([`quiddity_node::Peer::settled`]), saves, and shuts the network down
//! gracefully, so that what was sent is not lost in the closing. It knows
//! nothing about Tauri's windows, so it is tested on its own.

use std::path::Path;
use std::time::Duration;

use quiddity_node::SyncState;
use quiddity_node::keyring::Keyring;
use tracing::warn;

use crate::process::{self, Live};
use crate::running::Running;
use crate::session::{Error, Session};

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// There is no graph on this device yet, so there was nothing to sync.
    NoGraph,
    /// The state it was in at the end, which is what the caller decides
    /// whether to try again from: settled (see [`SyncState::is_settled`]), or
    /// `Offline` if nobody was reached, or `Behind` if it was cut off.
    Done(SyncState),
}

/// Syncs the graph in `dir` and stops, within `timeout`.
///
/// If a window has the graph open in this process it is used as it is, and
/// left running; it is only woken to try the devices it has been waiting to
/// retry. Otherwise the graph is opened and started here, and stopped before
/// returning unless a window took it meanwhile.
pub async fn sync_once(dir: &Path, keys: Keyring, timeout: Duration) -> Result<Outcome, Error> {
    sync_with(dir, keys, timeout, crate::start).await
}

/// [`sync_once`], told how to start a session: tests don't use the network.
async fn sync_with(
    dir: &Path,
    keys: Keyring,
    timeout: Duration,
    start: impl AsyncFnOnce(&Session, &Running),
) -> Result<Outcome, Error> {
    if let Some(live) = process::look_for(dir) {
        // The wait between attempts can be as long as a day.
        live.session.peer().retry_now();
        return Ok(Outcome::Done(live.session.peer().settled(timeout).await));
    }
    let Some(session) = Session::find(dir, keys)? else {
        return Ok(Outcome::NoGraph);
    };
    let running = Running::default();
    start(&session, &running).await;
    process::lend(Live {
        session: session.clone(),
        running,
    });
    let state = session.peer().settled(timeout).await;
    if let Err(e) = session.save() {
        warn!(error = %e, "couldn't save the graph after syncing");
    }
    if let Some(live) = process::release() {
        live.running.stop().await;
    }
    Ok(Outcome::Done(state))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(dir: &Path) -> Keyring {
        Keyring::new(dir.join("keys"))
    }

    async fn once(dir: &Path, timeout: Duration) -> Result<Outcome, Error> {
        sync_with(dir, keys(dir), timeout, offline).await
    }

    /// A start that puts nothing on the network.
    async fn offline(_: &Session, _: &Running) {}

    /// One test, since what is held is the process's.
    #[tokio::test]
    async fn it_opens_syncs_and_lets_go_unless_a_window_has_the_graph() {
        let _alone = process::TESTING.lock().await;
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("graph");

        // No graph is not made: a device must be told how to begin.
        assert_eq!(
            once(&dir, Duration::from_secs(5)).await.unwrap(),
            Outcome::NoGraph
        );
        assert!(!dir.join("graph.quiddity").exists());

        // A graph with nobody to sync with is done at once, and let go of.
        Session::create(&dir, keys(&dir), false).unwrap();
        assert_eq!(
            once(&dir, Duration::from_secs(5)).await.unwrap(),
            Outcome::Done(SyncState::Alone)
        );
        assert!(
            process::look_for(&dir).is_none(),
            "the run's session is let go"
        );

        // One a window has open is used and left held.
        let window = Session::open(&dir, keys(&dir)).unwrap();
        process::hold(Live {
            session: window.clone(),
            running: Running::default(),
        });
        window.peer().allow("somebody").unwrap();
        let started = std::time::Instant::now();
        let quick = once(&dir, Duration::from_millis(200)).await;
        assert_eq!(quick.unwrap(), Outcome::Done(SyncState::Offline));
        assert!(started.elapsed() >= Duration::from_millis(200));
        assert!(
            process::look_for(&dir).is_some(),
            "the window's is not let go"
        );
    }
}
