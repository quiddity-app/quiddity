//! The Tauri shell: opens the graph, hands it to the UI, keeps the UI's
//! edits safe, and shares the graph with the user's other devices.
//!
//! The UI holds its own replica of the graph (the core compiled to WASM), so
//! typing never waits on IPC. These commands carry the graph in as a snapshot
//! and the edits back as updates, both as raw bytes. The shell's replica is
//! the one on the network.

mod appearance;
mod library;
mod menu;
mod process;
mod running;
mod session;
mod updates;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use quiddity_node::events::Level;
use quiddity_node::keyring::Keyring;
use serde::Serialize;

use library::{Graphs, Library};
use running::Running;
use session::{Error, Recovery, Session, Start};
use tauri::ipc::{Channel, InvokeBody, InvokeResponseBody, Request, Response};
use tauri::{Manager, State};
use tokio::sync::oneshot;
use tracing::{error, warn};

type Shared<'a> = State<'a, Mutex<Session>>;

/// Why the graph couldn't be opened, and where it is, while it can't be: the
/// UI shows this instead of the graph, and offers a way on (`recover`).
#[derive(Debug, Clone, Serialize)]
struct Problem {
    dir: PathBuf,
    message: String,
}

/// Empty unless the graph couldn't be opened at startup. Always managed, so
/// the UI can ask.
#[derive(Default)]
struct Startup(Mutex<Option<Problem>>);

/// Where this device's graph will go, while it has none and has not been told
/// how to begin. Always managed, so the UI can ask. A device that has a graph
/// never has this set.
#[derive(Default)]
struct FirstRun(Mutex<Option<PathBuf>>);

/// Whether this device has no graph yet and is waiting to be told to start
/// one or join another's. The UI asks before anything else, as it does for
/// `startup_problem`.
#[tauri::command]
async fn first_run(first: State<'_, FirstRun>) -> Result<bool, Error> {
    Ok(first.0.lock().expect("first run lock poisoned").is_some())
}

/// Begins the graph on a device that has none, as the user chose. It does
/// nothing where there is a graph already: only a device that was waiting may
/// start one.
#[tauri::command]
async fn start_graph(
    app: tauri::AppHandle,
    first: State<'_, FirstRun>,
    running: State<'_, Running>,
    how: Start,
) -> Result<(), Error> {
    let dir = first
        .0
        .lock()
        .expect("first run lock poisoned")
        .clone()
        .ok_or(Error::NotFirstRun)?;
    let keys = match keyring(&app) {
        Ok(keys) => keys,
        Err(e) => return Err(Error::Keys(e.to_string())),
    };
    let session = begin(Session::create(&dir, keys, how == Start::Join)?, &running).await;
    app.manage(Mutex::new(session));
    *first.0.lock().expect("first run lock poisoned") = None;
    Ok(())
}

/// Whether this device is waiting for a graph from another device.
#[tauri::command]
async fn is_joining(state: Shared<'_>) -> Result<bool, Error> {
    Ok(state.lock().expect("session lock poisoned").joining())
}

/// What stopped the graph opening, if anything did. The UI asks first, and
/// shows it in place of the graph.
#[tauri::command]
async fn startup_problem(startup: State<'_, Startup>) -> Result<Option<Problem>, Error> {
    Ok(startup.0.lock().expect("startup lock poisoned").clone())
}

/// Goes on after `startup_problem`, as the user chose. It does nothing to a
/// graph that opened: starting again sets the files aside, and only a graph
/// that has already failed to open may be treated so.
#[tauri::command]
async fn recover(
    app: tauri::AppHandle,
    startup: State<'_, Startup>,
    running: State<'_, Running>,
    how: Recovery,
) -> Result<(), Error> {
    let dir = {
        let problem = startup.0.lock().expect("startup lock poisoned");
        problem.as_ref().ok_or(Error::NothingToRecover)?.dir.clone()
    };
    let keys = match keyring(&app) {
        Ok(keys) => keys,
        Err(e) => return Err(Error::Keys(e.to_string())),
    };
    match Session::recover(&dir, keys, how) {
        Ok(session) => {
            let session = begin(session, &running).await;
            app.manage(Mutex::new(session));
            *startup.0.lock().expect("startup lock poisoned") = None;
            Ok(())
        }
        Err(e) => {
            error!(error = %e, "still can't open the graph");
            *startup.0.lock().expect("startup lock poisoned") = Some(Problem {
                dir,
                message: e.to_string(),
            });
            Err(e)
        }
    }
}

/// The whole graph, for the UI to start from.
///
/// Commands are `async` so they run off the main thread. The lock is never
/// held across an await point, so a poisoned lock is the only way to fail
/// here, and that is a bug worth crashing for.
#[tauri::command]
async fn load_graph(state: Shared<'_>) -> Result<Response, Error> {
    let session = state.lock().expect("session lock poisoned");
    Ok(Response::new(session.snapshot()?))
}

/// Updates from the UI's edits, as the request body. The `x-typed` header
/// names the blocks they were typed in, comma separated, when that is all they
/// are: those are held back from devices that can't see them.
#[tauri::command]
async fn push_updates(state: Shared<'_>, request: Request<'_>) -> Result<(), Error> {
    let InvokeBody::Raw(updates) = request.body() else {
        return Err(Error::NotBytes);
    };
    let typed: Vec<String> = request
        .headers()
        .get("x-typed")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(',').map(str::to_owned).collect())
        .unwrap_or_default();
    state
        .lock()
        .expect("session lock poisoned")
        .apply(updates, &typed)
}

/// The blocks the user has open, which other devices see.
#[tauri::command]
async fn set_focus(state: Shared<'_>, blocks: Vec<String>) -> Result<(), Error> {
    state
        .lock()
        .expect("session lock poisoned")
        .set_focus(&blocks);
    Ok(())
}

/// Sends the UI what other devices change, as raw bytes on `channel`, for
/// as long as the UI listens. The UI imports them into its replica.
#[tauri::command]
async fn subscribe_updates(
    state: Shared<'_>,
    running: State<'_, Running>,
    channel: Channel<InvokeResponseBody>,
) -> Result<(), Error> {
    let session = state.lock().expect("session lock poisoned").clone();
    let task = tauri::async_runtime::spawn(async move {
        let mut changes = session.peer().subscribe();
        loop {
            match session.ui_updates() {
                Ok(Some(updates)) => {
                    // The UI has gone, such as when it reloaded: stop.
                    if channel.send(InvokeResponseBody::Raw(updates)).is_err() {
                        return;
                    }
                }
                Ok(None) => {}
                Err(e) => warn!(error = %e, "couldn't update the UI"),
            }
            if changes.changed().await.is_err() {
                return;
            }
        }
    });
    running.tasks().others.push(task);
    Ok(())
}

/// Tells the network the device may have moved: the page is shown again, after
/// the screen was off or the app in the background. A phone's sockets and relay
/// connection don't survive that, and iroh can't see it happen on every
/// system (Android's network changes aren't visible to native code), so without
/// this it dials on a dead socket until it gives up. It also starts the
/// dialler's waits again (`Peer::retry_now`), so a device that was out of reach
/// is tried now. It does no harm when nothing changed, and nothing when the
/// network is off.
#[tauri::command]
async fn network_changed(app: tauri::AppHandle, running: State<'_, Running>) -> Result<(), Error> {
    let endpoint = running
        .tasks()
        .endpoint
        .lock()
        .expect("endpoint lock poisoned")
        .clone();
    if let Some(endpoint) = endpoint {
        endpoint.network_change().await;
    }
    // A device that was out of reach is dialled now, not when its delay (which
    // grows to a day) is over. There is no session before a graph is chosen.
    if let Some(session) = app.try_state::<Mutex<Session>>() {
        session
            .lock()
            .expect("session lock poisoned")
            .peer()
            .retry_now();
    }
    Ok(())
}

/// Tells the UI how this device stands with the others, now and each time it
/// changes, as `Session::sync_details` says.
#[tauri::command]
async fn subscribe_status(
    state: Shared<'_>,
    running: State<'_, Running>,
    channel: Channel<session::SyncDetails>,
) -> Result<(), Error> {
    let session = state.lock().expect("session lock poisoned").clone();
    let task = tauri::async_runtime::spawn(async move {
        let mut changes = session.peer().subscribe_status();
        let mut told = None;
        loop {
            let now = session.sync_details();
            // The UI has gone, such as when it reloaded: stop.
            if told.as_ref() != Some(&now) && channel.send(now.clone()).is_err() {
                return;
            }
            told = Some(now);
            if changes.changed().await.is_err() {
                return;
            }
        }
    });
    running.tasks().others.push(task);
    Ok(())
}

/// This device's ID, for letting it in from another device.
#[tauri::command]
async fn device_id(state: Shared<'_>) -> Result<String, Error> {
    state.lock().expect("session lock poisoned").device_id()
}

/// Lets another device connect: the ID that device shows.
#[tauri::command]
async fn allow_device(state: Shared<'_>, device: String) -> Result<(), Error> {
    state.lock().expect("session lock poisoned").allow(&device)
}

/// Whether the UI has an entity open, which is what Delete Entity in the File
/// menu needs. There is no such menu on other systems, and it then does nothing.
#[tauri::command]
async fn set_entity_open(app: tauri::AppHandle, open: bool) -> Result<(), Error> {
    menu::set_entity_open(&app, open);
    Ok(())
}

/// Keeps the bytes of a file, the request body, and returns their hash. The
/// UI then says in the graph what the file is, as it does any edit.
#[tauri::command]
async fn put_blob(state: Shared<'_>, request: Request<'_>) -> Result<String, Error> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(Error::NotBytes);
    };
    let session = state.lock().expect("session lock poisoned").clone();
    session.put_blob(bytes.clone()).await
}

/// The bytes of a file the graph names, by hash, as raw bytes. The UI shows
/// them from an object URL.
#[tauri::command]
async fn get_blob(state: Shared<'_>, hash: String) -> Result<Response, Error> {
    let session = state.lock().expect("session lock poisoned").clone();
    Ok(Response::new(session.blob(&hash).await?))
}

/// Which graphs there are and which is open, for the UI's picker.
#[tauri::command]
async fn list_graphs(state: Shared<'_>, library: State<'_, Library>) -> Result<Graphs, Error> {
    let dir = state
        .lock()
        .expect("session lock poisoned")
        .store()
        .dir()
        .to_owned();
    library.list(&dir)
}

/// Opens `session`'s graph in place of the open one, which is saved and
/// stopped first. The UI starts over from the new graph, as it does after
/// recovering. If the new graph can't be opened, nothing has changed.
async fn replace_session(
    state: &Shared<'_>,
    running: &Running,
    library: &Library,
    name: &str,
    session: Session,
) -> Result<(), Error> {
    let old = state.lock().expect("session lock poisoned").clone();
    // A moment's typing may not be saved yet.
    if let Err(e) = old.save() {
        warn!(error = %e, "couldn't save the graph being left");
    }
    running.stop().await;
    let session = begin(session, running).await;
    *state.lock().expect("session lock poisoned") = session;
    library.choose(name)
}

/// Opens another graph on this device.
#[tauri::command]
async fn switch_graph(
    app: tauri::AppHandle,
    state: Shared<'_>,
    running: State<'_, Running>,
    library: State<'_, Library>,
    name: String,
) -> Result<(), Error> {
    let dir = library.existing(&name)?;
    if dir == state.lock().expect("session lock poisoned").store().dir() {
        return Ok(());
    }
    let keys = keyring(&app).map_err(|e| Error::Keys(e.to_string()))?;
    let session = Session::open(&dir, keys)?;
    replace_session(&state, &running, &library, &name, session).await
}

/// Makes a graph and opens it: empty and of its own, or (`sync`) blank, to be
/// filled by syncing with another device that has the graph.
#[tauri::command]
async fn create_graph(
    app: tauri::AppHandle,
    state: Shared<'_>,
    running: State<'_, Running>,
    library: State<'_, Library>,
    name: String,
    sync: bool,
) -> Result<(), Error> {
    let (name, dir) = library.fresh(&name)?;
    let keys = keyring(&app).map_err(|e| Error::Keys(e.to_string()))?;
    let session = Session::create(&dir, keys, sync)?;
    replace_session(&state, &running, &library, &name, session).await
}

/// Puts the session on the network, and keeps it saved as other devices
/// change it. `QUIDDITY_NO_SYNC` leaves it off, and `QUIDDITY_LOCAL` (an
/// address) keeps it to that address, with no relays or lookup: for trying
/// it between two devices on a network, or on one machine. With no lookup a
/// device can only be dialled where it is told to be: `QUIDDITY_PEERS` is
/// `ID=ADDRESS`, comma separated.
fn start_network(session: &Session, running: &Running) {
    if std::env::var_os("QUIDDITY_NO_SYNC").is_some() {
        session.peer().record(
            Level::Problem,
            None,
            "the network is off: QUIDDITY_NO_SYNC is set".to_owned(),
        );
        return;
    }
    let local = std::env::var("QUIDDITY_LOCAL")
        .ok()
        .and_then(|address| address.parse().ok());
    // Where some devices are, for where there is no lookup to find them.
    let known = match std::env::var("QUIDDITY_PEERS") {
        Ok(text) => quiddity_node::known_addresses(&text).unwrap_or_else(|e| {
            warn!(error = %e, "ignoring QUIDDITY_PEERS");
            HashMap::default()
        }),
        Err(_) => HashMap::default(),
    };
    let (peer, store) = (session.peer().clone(), session.store().clone());
    let (stop, stopped) = oneshot::channel();
    let slot = Arc::clone(&running.tasks().endpoint);
    let task = tauri::async_runtime::spawn(async move {
        let started = match store.key() {
            Ok(key) => quiddity_node::serve_at(&peer, key, local, known)
                .await
                .map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        match started {
            Ok(server) => {
                peer.record(
                    Level::Info,
                    None,
                    format!(
                        "on the network as {}{}",
                        server.endpoint().id(),
                        if local.is_some() { " (local only)" } else { "" }
                    ),
                );
                let saver = quiddity_node::save_on_change(peer, store);
                *slot.lock().expect("endpoint lock poisoned") = Some(server.endpoint().clone());
                // The endpoint lives until the graph is switched away from
                // (or the app ends, which drops the sender).
                let _ = stopped.await;
                slot.lock().expect("endpoint lock poisoned").take();
                saver.abort();
                if let Err(e) = server.shutdown().await {
                    warn!(error = %e, "couldn't close the endpoint");
                }
            }
            Err(e) => {
                error!(error = %e, "not syncing");
                peer.record(
                    Level::Problem,
                    None,
                    format!("couldn't join the network: {e}"),
                );
            }
        }
    });
    let mut tasks = running.tasks();
    tasks.stop = Some(stop);
    tasks.network = Some(task);
}

/// Gets an opened graph going: keeps its binaries, then puts it on the network.
async fn begin(session: Session, running: &Running) -> Session {
    // Before the network, which gives and fetches what this keeps. A
    // store that won't open leaves the app without binaries, no more.
    if let Err(e) = session.keep_blobs().await {
        warn!(error = %e, "not keeping binaries");
    }
    start_network(&session, running);
    // So that a window made again in this process finds it (`process.rs`).
    process::hold(process::Live {
        session: session.clone(),
        running: running.clone(),
    });
    session
}

/// Where this device keeps its key, which is not in any graph: `QUIDDITY_CONFIG_DIR`
/// if it is set, which is how two instances on one machine can be two devices,
/// and otherwise the app's own configuration directory.
fn keyring(app: &tauri::AppHandle) -> Result<Keyring, Box<dyn std::error::Error>> {
    match std::env::var_os("QUIDDITY_CONFIG_DIR") {
        Some(dir) => Ok(Keyring::new(dir)),
        None => Ok(Keyring::new(app.path().app_config_dir()?)),
    }
}

/// Where the graphs are: `QUIDDITY_GRAPH` names one graph's directory, for
/// working on a graph made with `quiddity-node`, and then it is the only graph;
/// otherwise they are in the app's data directory.
fn library(app: &tauri::App) -> Result<Library, Box<dyn std::error::Error>> {
    Ok(Library::new(
        app.path().app_data_dir()?,
        std::env::var_os("QUIDDITY_GRAPH").map(PathBuf::from),
    ))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[allow(
    clippy::exit,
    reason = "`tauri::generate_context!` expands to an `exit` on some toolchains"
)]
pub fn run() {
    // `QUIDDITY_LOG` is a level (`debug`, say); the default is warnings.
    let level = std::env::var("QUIDDITY_LOG")
        .ok()
        .and_then(|level| level.parse().ok())
        .unwrap_or(tracing::level_filters::LevelFilter::WARN);
    // The entry point can run again in a process that goes on (see
    // `process.rs`), and a second logger would be refused: keep the first.
    drop(
        tracing_subscriber::fmt()
            .with_max_level(level)
            .with_writer(std::io::stderr)
            .try_init(),
    );
    let builder = tauri::Builder::default()
        // Opens links from the text of a block in the system's browser.
        .plugin(tauri_plugin_opener::init());
    // Updates replace the running binary, which a phone doesn't allow.
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::Pending::default());
    // The menu bar only tells the page what was chosen.
    #[cfg(target_os = "macos")]
    let builder = builder.on_menu_event(menu::on_event);
    builder
        .setup(|app| {
            let library = library(app)?;
            let dir = library.start()?;
            app.manage(library);
            let keys = keyring(app.handle())?;
            let mut waiting = None;
            let problem = if let Some(live) = process::held_for(&dir) {
                // The window is new but the process isn't: the graph is open
                // and on the network already, and opening it again would take
                // what the first holds.
                app.manage(live.running);
                app.manage(Mutex::new(live.session));
                None
            } else {
                app.manage(Running::default());
                // A graph that can't be opened is the UI's to deal with, with a
                // window to do it in: exiting here would leave nothing on screen.
                match Session::find(&dir, keys) {
                    Ok(Some(session)) => {
                        let running = app.state::<Running>();
                        app.manage(Mutex::new(tauri::async_runtime::block_on(begin(
                            session, &running,
                        ))));
                        None
                    }
                    // No graph yet: the UI asks how to begin (`first_run`).
                    Ok(None) => {
                        waiting = Some(dir);
                        None
                    }
                    Err(e) => {
                        error!(error = %e, dir = %dir.display(), "can't open the graph");
                        Some(Problem {
                            dir,
                            message: e.to_string(),
                        })
                    }
                }
            };
            app.manage(Startup(Mutex::new(problem)));
            app.manage(FirstRun(Mutex::new(waiting)));
            if let Some(window) = app.get_webview_window("main") {
                appearance::follow_theme(&window);
            }
            #[cfg(target_os = "macos")]
            menu::install(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            startup_problem,
            recover,
            list_graphs,
            switch_graph,
            create_graph,
            first_run,
            start_graph,
            is_joining,
            load_graph,
            push_updates,
            set_focus,
            subscribe_updates,
            subscribe_status,
            network_changed,
            device_id,
            allow_device,
            put_blob,
            get_blob,
            set_entity_open,
            updates::check_for_update,
            updates::install_update
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Quiddity app");
}
