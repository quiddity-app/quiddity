//! Syncing once, for a run that has a few minutes and then goes: on Android, a
//! job the system starts, possibly in a process the app isn't running in.
//!
//! It opens the graph if nothing has, puts it on the network, waits for the
//! devices it can reach to have what it has and the other way round
//! ([`quiddity_node::Peer::settled`]), saves, and shuts the network down
//! gracefully, so that what was sent is not lost in the closing. It knows
//! nothing about Tauri's windows, so it is tested on its own.

use std::path::{Path, PathBuf};
use std::time::Duration;

use quiddity_node::SyncState;
use quiddity_node::keyring::Keyring;
use tracing::warn;

use crate::library::Library;
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

impl Outcome {
    /// The outcome as the number the Android plugin reads (`Native.kt` has the
    /// same numbers): `SETTLED` when it is done, `OFFLINE` when nobody was
    /// reached and `BEHIND` when it was cut off, which are both worth another
    /// try before long, and `NO_GRAPH`.
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::Done(state) if state.is_settled() => SETTLED,
            Self::Done(SyncState::Behind) => BEHIND,
            Self::Done(_) => OFFLINE,
            Self::NoGraph => NO_GRAPH,
        }
    }
}

pub const SETTLED: i32 = 0;
pub const OFFLINE: i32 = 1;
pub const BEHIND: i32 = 2;
pub const NO_GRAPH: i32 = 3;
/// The run itself failed: the graph wouldn't open, say.
pub const FAILED: i32 = 4;

/// Where Tauri puts the app's data and its configuration on Android: both are
/// the app's data directory (`Context.dataDir`) and then the app's identifier
/// (`Context.packageName`, which `tauri android init` makes the identifier). A
/// run with no window has no Tauri to ask, so it works them out; **the app
/// checks it against what Tauri says at startup**, so a change in Tauri's
/// layout is logged, and a wrong place finds no graph (`NoGraph`) and never
/// makes one.
#[must_use]
pub fn android_dir(data_dir: &Path, identifier: &str) -> PathBuf {
    data_dir.join(identifier)
}

/// Syncs the graph the app has open on this device (the one in `data_dir` that
/// the app chose last, as its startup does), keeping the device's key in
/// `config_dir`. See [`sync_once`].
pub async fn sync_in(
    data_dir: &Path,
    config_dir: &Path,
    timeout: Duration,
    blobs: bool,
) -> Result<Outcome, Error> {
    sync_in_with(data_dir, config_dir, timeout, async |session, running| {
        crate::start(session, running, blobs).await;
    })
    .await
}

/// [`sync_in`], told how to start a session: tests don't use the network.
async fn sync_in_with(
    data_dir: &Path,
    config_dir: &Path,
    timeout: Duration,
    start: impl AsyncFnOnce(&Session, &Running),
) -> Result<Outcome, Error> {
    let dir = Library::new(data_dir.to_owned(), None).start()?;
    sync_with(&dir, Keyring::new(config_dir), timeout, start).await
}

/// Whether a sync is under way that the user shouldn't interrupt: the live
/// session, if there is one, has a peer with something to fetch or typing to
/// send. The Android plugin asks it when the user leaves the app, to decide
/// whether to hold a short foreground service.
#[must_use]
pub fn outstanding() -> bool {
    process::any().is_some_and(|live| live.session.peer().sync_state() == SyncState::Behind)
}

/// Syncs the graph in `dir` and stops, within `timeout`.
///
/// If a window has the graph open in this process it is used as it is, and
/// left running; it is only woken to try the devices it has been waiting to
/// retry. Otherwise the graph is opened and started here, and stopped before
/// returning unless a window took it meanwhile. Without `blobs` the graph is
/// synced and the files are not (for a metered network).
pub async fn sync_once(
    dir: &Path,
    keys: Keyring,
    timeout: Duration,
    blobs: bool,
) -> Result<Outcome, Error> {
    sync_with(dir, keys, timeout, async |session, running| {
        crate::start(session, running, blobs).await;
    })
    .await
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

    /// The numbers the Kotlin side reads (`Native.kt`), which a change here
    /// must change there.
    #[test]
    fn outcomes_are_the_numbers_the_android_plugin_reads() {
        let cases = [
            (Outcome::Done(SyncState::Alone), 0),
            (Outcome::Done(SyncState::Synced), 0),
            (Outcome::Done(SyncState::Present), 0),
            (Outcome::Done(SyncState::Offline), 1),
            (Outcome::Done(SyncState::Behind), 2),
            (Outcome::NoGraph, 3),
        ];
        for (outcome, code) in cases {
            assert_eq!(outcome.code(), code, "{outcome:?}");
        }
        assert_eq!(
            [SETTLED, OFFLINE, BEHIND, NO_GRAPH, FAILED],
            [0, 1, 2, 3, 4]
        );
    }

    #[test]
    fn android_keeps_data_and_config_in_the_data_directory_under_the_identifier() {
        assert_eq!(
            android_dir(Path::new("/data/user/0/uk.co.example"), "uk.co.example"),
            Path::new("/data/user/0/uk.co.example/uk.co.example")
        );
    }

    /// The graph the app chose is the one synced, found as its startup does,
    /// and none is made where there is none.
    #[tokio::test]
    async fn it_syncs_the_graph_the_app_has_in_its_data_directory() {
        let _alone = process::TESTING.lock().await;
        let root = tempfile::tempdir().unwrap();
        let (data, config) = (root.path().join("data"), root.path().join("config"));
        let sync = |timeout| sync_in_with(&data, &config, timeout, offline);
        let quick = Duration::from_millis(200);

        assert_eq!(sync(quick).await.unwrap(), Outcome::NoGraph);
        assert!(!data.join("graphs").exists(), "nothing was made");

        let graph = data.join("graphs").join("Quiddity");
        Session::create(&graph, Keyring::new(&config), false).unwrap();
        assert_eq!(
            sync(quick).await.unwrap(),
            Outcome::Done(SyncState::Alone),
            "found where the app keeps it, with the key where it keeps that"
        );
    }

    /// Nothing is outstanding for a session that is alone, or for none.
    #[tokio::test]
    async fn nothing_is_outstanding_unless_the_live_session_is_behind() {
        let _alone = process::TESTING.lock().await;
        process::forget();
        assert!(!outstanding(), "no session");

        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("graph");
        let session = Session::open(&dir, keys(&dir)).unwrap();
        process::hold(Live {
            session,
            running: Running::default(),
        });
        assert!(!outstanding(), "alone");
        process::forget();
    }

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
