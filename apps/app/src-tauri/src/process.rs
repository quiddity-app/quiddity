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

static LIVE: Mutex<Option<Live>> = Mutex::new(None);

fn live() -> MutexGuard<'static, Option<Live>> {
    LIVE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Keeps `live` as the one that is running, in place of any before it: it is
/// called each time a session begins, so switching graphs changes it too.
pub fn hold(live_now: Live) {
    *live() = Some(live_now);
}

/// The live session, if it is the graph in `dir`. A different graph is none:
/// whatever is asking will open its own, as it would have without this.
pub fn held_for(dir: &Path) -> Option<Live> {
    live()
        .clone()
        .filter(|held| held.session.store().dir() == dir)
}

#[cfg(test)]
mod tests {
    use quiddity_node::keyring::Keyring;

    use super::*;

    fn session(dir: &Path) -> Session {
        Session::open(dir, Keyring::new(dir.join("keys"))).unwrap()
    }

    /// One test, since what it holds is the process's.
    #[test]
    fn a_session_is_found_again_for_its_graph_and_replaced_when_another_begins() {
        let (first, second) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (a, b) = (first.path().join("a"), second.path().join("b"));
        assert!(held_for(&a).is_none(), "nothing is held to begin with");

        let running = Running::default();
        let original = session(&a);
        hold(Live {
            session: original.clone(),
            running: running.clone(),
        });
        let found = held_for(&a).expect("the graph that is open");
        // The same graph, not another opened from the file.
        found
            .session
            .peer()
            .graph()
            .create_block(None, None, "typed")
            .unwrap();
        assert_eq!(original.peer().graph().blocks().len(), 1);
        assert!(held_for(&b).is_none(), "another graph is not it");

        hold(Live {
            session: session(&b),
            running,
        });
        assert!(held_for(&a).is_none(), "the one before was replaced");
        assert!(held_for(&b).is_some());
    }
}
