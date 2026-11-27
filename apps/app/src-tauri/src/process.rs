//! The session that is live, kept for as long as the process is.
//!
//! The graph's file and the device's endpoint can each be held once, but the
//! window that shows the graph can be made again while the process goes on:
//! on Android the activity is destroyed and recreated (turning the screen, say)
//! without the process ending, and the app's entry point runs again. Setting
//! up from scratch then would open the graph a second time, beside the first
//! that is still on the network, so the first is kept here and found again.
//!
//! It knows nothing about Tauri, like `session.rs`, so it is tested on its own.

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::running::Running;
use crate::session::Session;

/// An open graph and what runs for it.
#[derive(Clone)]
pub struct Live {
    pub session: Session,
    pub running: Running,
}

/// The live session, and whether a window has taken it. One a background run
/// started and no window has taken is the run's to stop when it is done; once
/// a window has it, it is the window's, and stopping it would leave the window
/// without a network.
struct Held {
    live: Live,
    taken: bool,
}

static LIVE: Mutex<Option<Held>> = Mutex::new(None);

fn held() -> MutexGuard<'static, Option<Held>> {
    LIVE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Keeps `live` as the one that is running, for a window: it is called each
/// time a session begins there, so switching graphs changes it too.
pub fn hold(live: Live) {
    *held() = Some(Held { live, taken: true });
}

/// Keeps `live` as the one that is running, for a background run, which stops
/// it when it is done unless a window has taken it by then (`release`).
pub fn lend(live: Live) {
    *held() = Some(Held { live, taken: false });
}

/// The live session, if it is the graph in `dir`, for a window to take. A
/// different graph is none: whatever is asking will open its own, as it would
/// have without this.
pub fn take_for(dir: &Path) -> Option<Live> {
    let mut held = held();
    let held = held
        .as_mut()
        .filter(|h| h.live.session.store().dir() == dir)?;
    held.taken = true;
    Some(held.live.clone())
}

/// The live session, if it is the graph in `dir`, for a background run to
/// sync with. It is not taken: whoever started it still owns it.
pub fn look_for(dir: &Path) -> Option<Live> {
    held()
        .as_ref()
        .filter(|h| h.live.session.store().dir() == dir)
        .map(|h| h.live.clone())
}

/// The live session, whatever graph it is: for asking how it stands.
pub fn any() -> Option<Live> {
    held().as_ref().map(|h| h.live.clone())
}

/// Lets go of what a background run lent, once it is done, and returns it to
/// be stopped. A window that took it meanwhile keeps it, and this returns none.
pub fn release() -> Option<Live> {
    let mut held = held();
    if held.as_ref()?.taken {
        return None;
    }
    held.take().map(|h| h.live)
}

/// Forgets what is held, for a test that needs there to be nothing.
#[cfg(test)]
pub fn forget() {
    *held() = None;
}

/// Tests that use what is held take this first: it is the process's, and
/// they run side by side. Async, so that a test can hold it across an await.
#[cfg(test)]
pub static TESTING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
mod tests {
    use quiddity_node::keyring::Keyring;

    use super::*;

    fn live(dir: &Path) -> Live {
        let session = Session::open(dir, Keyring::new(dir.join("keys"))).unwrap();
        Live {
            session,
            running: Running::default(),
        }
    }

    /// One test, since what it holds is the process's.
    #[test]
    fn what_is_held_is_found_for_its_graph_and_lent_only_until_a_window_takes_it() {
        let _alone = TESTING.blocking_lock();
        let (first, second) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (a, b) = (first.path().join("a"), second.path().join("b"));
        assert!(take_for(&a).is_none() && look_for(&a).is_none() && release().is_none());

        // A window's session is found again, and is the same graph, not
        // another opened from the file.
        let original = live(&a);
        hold(original.clone());
        let found = take_for(&a).expect("the graph that is open");
        found
            .session
            .peer()
            .graph()
            .create_block(None, None, "typed")
            .unwrap();
        assert_eq!(original.session.peer().graph().blocks().len(), 1);
        assert!(take_for(&b).is_none(), "another graph is not it");
        assert!(look_for(&b).is_none());
        assert!(release().is_none(), "a window's is not the run's to stop");
        assert!(look_for(&a).is_some(), "and it is still held");

        // A later one replaces it, and a run's is let go when it is done...
        lend(live(&b));
        assert!(take_for(&a).is_none(), "the one before was replaced");
        assert!(look_for(&b).is_some(), "looking doesn't take it");
        assert!(release().is_some());
        assert!(look_for(&b).is_none(), "and nothing is held after");

        // ...unless a window took it while the run was going.
        lend(live(&b));
        assert!(take_for(&b).is_some());
        assert!(release().is_none());
        assert!(look_for(&b).is_some());
    }
}
