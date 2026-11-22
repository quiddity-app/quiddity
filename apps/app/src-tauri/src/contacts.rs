//! Keeping the phone's contacts in step with the graph, while the app runs.
//!
//! The graph is in this process and nowhere else, so the contacts can only be
//! reconciled while the app is running: when the graph changes, when the user
//! has turned contacts on, and now and then to look for an edit made in the
//! contacts app. `quiddity-contacts` does the reconciling; this decides when,
//! and knows nothing of Tauri or Android so that it can be tested on its own.

// Started only on Android, where there are contacts to keep: it is all tested
// natively all the same.
#![cfg_attr(not(target_os = "android"), allow(dead_code))]

use std::sync::Arc;
use std::time::Duration;

use quiddity_contacts::{Report, Store};
use tokio::sync::Notify;
use tracing::{debug, warn};

use crate::session::{Error, Session};

/// How often the contacts app is looked at for an edit, when nothing else has
/// woken the loop. It is a cheap question: which rows are dirty.
pub const POLL: Duration = Duration::from_secs(5);

/// How long the graph is left alone after a change before the contacts are
/// reconciled, so that typing is one reconcile and not one a keystroke.
pub const QUIET: Duration = Duration::from_secs(2);

/// One go at it. Nothing if contacts are off. If `full` (the graph changed,
/// or contacts were just turned on) the whole phone is reconciled, and if not,
/// only when the phone has an edit, which is what a poll is looking for.
pub fn step<S: Store>(
    session: &Session,
    store: &mut S,
    enabled: bool,
    full: bool,
) -> Result<Option<Report>, Error> {
    if !enabled {
        return Ok(None);
    }
    if !full {
        let edited = store.edited().map_err(|e| Error::Contacts(e.to_string()))?;
        if edited.is_empty() {
            return Ok(None);
        }
    }
    session.reconcile_contacts(store).map(Some)
}

/// Runs `go` (a step, told whether it is a full one) each time the graph
/// changes, `wake` is notified, or `poll` has passed. It ends when the
/// session's peer does.
pub async fn run<F>(session: Session, wake: Arc<Notify>, poll: Duration, quiet: Duration, go: F)
where
    F: Fn(bool) -> Result<Option<Report>, Error> + Send + Sync + 'static,
{
    let go = Arc::new(go);
    let mut changes = session.peer().subscribe();
    // The first one is full: there may be contacts the phone lacks.
    let mut full = true;
    loop {
        // What changed before now is dealt with by what follows. A change
        // during it, including the step's own, is not: it makes another.
        changes.borrow_and_update();
        let attempt = {
            let go = go.clone();
            tokio::task::spawn_blocking(move || go(full)).await
        };
        match attempt {
            Ok(Ok(Some(report))) => debug!(?report, "reconciled the contacts"),
            Ok(Ok(None)) => {}
            Ok(Err(e)) => warn!(error = %e, "couldn't reconcile the contacts"),
            Err(e) => warn!(error = %e, "reconciling the contacts stopped"),
        }
        tokio::select! {
            changed = changes.changed() => {
                if changed.is_err() {
                    return;
                }
                // Let a burst of changes finish.
                tokio::time::sleep(quiet).await;
                full = true;
            }
            () = wake.notified() => full = true,
            () = tokio::time::sleep(poll) => full = false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use quiddity_contacts::{Contact, Edited, Entry, Held};
    use quiddity_core::Graph;
    use quiddity_core::contact::ContactField;
    use quiddity_node::keyring::Keyring;

    use super::*;

    fn open(dir: &Path) -> Session {
        Session::open(dir, Keyring::new(dir.join("keys"))).unwrap()
    }

    /// A session whose graph has a `mobile` property that fills a phone
    /// number, and a Jane with one.
    fn with_jane(dir: &Path) -> Session {
        let session = open(dir);
        let ui = Graph::from_snapshot(&session.snapshot().unwrap()).unwrap();
        let before = ui.version();
        let mobile = ui.create_block(None, None, "mobile").unwrap();
        ui.create_block(Some(mobile), None, "##contact(phone:mobile)")
            .unwrap();
        ui.create_entity(
            "Jane",
            &[(
                "mobile",
                Some(quiddity_core::syntax::Value::Text("111".into())),
            )],
        )
        .unwrap();
        ui.commit();
        session
            .apply(&ui.updates_since(&before).unwrap(), &[])
            .unwrap();
        session
    }

    #[derive(Default)]
    struct Phone {
        edited: Vec<Edited>,
        puts: Vec<Contact>,
        asked: usize,
    }

    impl Store for Phone {
        type Error = Infallible;

        fn edited(&mut self) -> Result<Vec<Edited>, Infallible> {
            self.asked += 1;
            // Rows stay dirty until they are put.
            Ok(self.edited.clone())
        }

        fn held(&mut self) -> Result<Vec<Held>, Infallible> {
            self.asked += 1;
            Ok(vec![])
        }

        fn put(&mut self, _: Option<&str>, contact: &Contact) -> Result<String, Infallible> {
            self.edited.clear();
            self.puts.push(contact.clone());
            Ok(format!("row{}", self.puts.len()))
        }

        fn remove(&mut self, _: &str) -> Result<(), Infallible> {
            Ok(())
        }
    }

    #[test]
    fn nothing_happens_while_contacts_are_off() {
        let dir = tempfile::tempdir().unwrap();
        let session = with_jane(dir.path());
        let mut phone = Phone::default();
        assert!(step(&session, &mut phone, false, true).unwrap().is_none());
        assert_eq!((phone.asked, phone.puts.len()), (0, 0));
    }

    #[test]
    fn a_poll_looks_only_for_edits() {
        let dir = tempfile::tempdir().unwrap();
        let session = with_jane(dir.path());
        let mut phone = Phone::default();
        assert!(step(&session, &mut phone, true, false).unwrap().is_none());
        assert_eq!(
            (phone.asked, phone.puts.len()),
            (1, 0),
            "it asked which rows are dirty, and nothing more"
        );

        let full = step(&session, &mut phone, true, true).unwrap().unwrap();
        assert_eq!(full.put, 1);
        assert_eq!(phone.puts[0].name, "Jane");
    }

    #[test]
    fn an_edit_on_the_phone_is_kept_and_sent_on() {
        let dir = tempfile::tempdir().unwrap();
        let session = with_jane(dir.path());
        let mut phone = Phone::default();
        step(&session, &mut phone, true, true).unwrap();
        let jane = phone.puts[0].clone();
        // The UI has all there is.
        session.snapshot().unwrap();
        assert!(session.ui_updates().unwrap().is_none());

        let mut edited = jane.clone();
        edited.entries.push(Entry {
            field: ContactField::Phone,
            label: Some("mobile".into()),
            value: "222".into(),
        });
        phone.edited = vec![Edited {
            row: "row1".into(),
            id: Some(jane.id.clone()),
            base: Some(jane),
            contact: edited,
            deleted: false,
        }];
        let report = step(&session, &mut phone, true, false).unwrap().unwrap();

        assert!(report.changed);
        assert!(
            session.ui_updates().unwrap().is_some(),
            "the UI is told, as of an edit from another device"
        );
        drop(session);
        let reopened = open(dir.path());
        let graph = Graph::from_snapshot(&reopened.snapshot().unwrap()).unwrap();
        let index = quiddity_core::Index::build(&graph).unwrap();
        let jane = &quiddity_contacts::contacts(&index)[0];
        assert_eq!(jane.entries.len(), 2, "saved: {jane:?}");
    }

    #[tokio::test]
    async fn the_loop_goes_on_a_wake_a_change_and_a_poll() {
        let dir = tempfile::tempdir().unwrap();
        let session = with_jane(dir.path());
        let wake = Arc::new(Notify::new());
        let (full, polled) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let (f, p) = (full.clone(), polled.clone());
        let looping = tokio::spawn(run(
            session.clone(),
            wake.clone(),
            Duration::from_millis(40),
            Duration::from_millis(5),
            move |is_full| {
                let counter = if is_full { &f } else { &p };
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(None)
            },
        ));
        let until = |what: &'static str, done: Box<dyn Fn() -> bool>| async move {
            for _ in 0..200 {
                if done() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            panic!("{what}");
        };

        let (f2, p2) = (full.clone(), polled.clone());
        until(
            "it starts with a full go",
            Box::new(move || f2.load(Ordering::SeqCst) >= 1),
        )
        .await;
        until("it polls", Box::new(move || p2.load(Ordering::SeqCst) >= 2)).await;

        let before = full.load(Ordering::SeqCst);
        wake.notify_one();
        let f3 = full.clone();
        until(
            "being woken is a full go",
            Box::new(move || f3.load(Ordering::SeqCst) > before),
        )
        .await;

        let before = full.load(Ordering::SeqCst);
        session.peer().changed();
        let f4 = full.clone();
        until(
            "a change is a full go",
            Box::new(move || f4.load(Ordering::SeqCst) > before),
        )
        .await;
        looping.abort();
    }
}
