//! The Tauri shell: opens the graph, hands it to the UI, keeps the UI's
//! edits safe, and shares the graph with the user's other devices.
//!
//! The UI holds its own replica of the graph (the core compiled to WASM), so
//! typing never waits on IPC. These commands carry the graph in as a snapshot
//! and the edits back as updates, both as raw bytes. The shell's replica is
//! the one on the network.

mod session;

use std::path::PathBuf;
use std::sync::Mutex;

use session::{Error, Session};
use tauri::ipc::{Channel, InvokeBody, InvokeResponseBody, Request, Response};
use tauri::{Manager, State};

type Shared<'a> = State<'a, Mutex<Session>>;

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
    channel: Channel<InvokeResponseBody>,
) -> Result<(), Error> {
    let session = state.lock().expect("session lock poisoned").clone();
    tauri::async_runtime::spawn(async move {
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
                Err(e) => eprintln!("quiddity: couldn't update the UI: {e}"),
            }
            if changes.changed().await.is_err() {
                return;
            }
        }
    });
    Ok(())
}

/// Tells the UI how this device stands with the others, now and each time it
/// changes, as the words `Session::sync_state` uses.
#[tauri::command]
async fn subscribe_status(state: Shared<'_>, channel: Channel<String>) -> Result<(), Error> {
    let session = state.lock().expect("session lock poisoned").clone();
    tauri::async_runtime::spawn(async move {
        let mut changes = session.peer().subscribe_status();
        let mut told = "";
        loop {
            let now = session.sync_state();
            // The UI has gone, such as when it reloaded: stop.
            if now != told && channel.send(now.to_owned()).is_err() {
                return;
            }
            told = now;
            if changes.changed().await.is_err() {
                return;
            }
        }
    });
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

/// Puts the session on the network, and keeps it saved as other devices
/// change it. `QUIDDITY_NO_SYNC` leaves it off, and `QUIDDITY_LOCAL` (an
/// address) keeps it to that address, with no relays or lookup: for trying
/// it between two devices on a network, or on one machine.
fn start_network(session: &Session) {
    if std::env::var_os("QUIDDITY_NO_SYNC").is_some() {
        return;
    }
    let local = std::env::var("QUIDDITY_LOCAL")
        .ok()
        .and_then(|address| address.parse().ok());
    let (peer, store) = (session.peer().clone(), session.store().clone());
    tauri::async_runtime::spawn(async move {
        let started = match store.key() {
            Ok(key) => quiddity_node::serve(&peer, key, local)
                .await
                .map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        match started {
            Ok(server) => {
                quiddity_node::save_on_change(peer, store);
                // The endpoint lives as long as the app.
                let _server = server;
                std::future::pending::<()>().await;
            }
            Err(e) => eprintln!("quiddity: not syncing: {e}"),
        }
    });
}

/// `QUIDDITY_GRAPH` names the graph's directory, for working on a graph made
/// with `quiddity-node`; otherwise there is one graph in the app's data
/// directory.
fn graph_dir(app: &tauri::App) -> Result<PathBuf, Box<dyn std::error::Error>> {
    match std::env::var_os("QUIDDITY_GRAPH") {
        Some(dir) => Ok(dir.into()),
        None => Ok(app.path().app_data_dir()?.join("graph")),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Opens links from the text of a block in the system's browser.
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let session = Session::open(&graph_dir(app)?)?;
            start_network(&session);
            app.manage(Mutex::new(session));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_graph,
            push_updates,
            set_focus,
            subscribe_updates,
            subscribe_status,
            device_id,
            allow_device
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Quiddity app");
}
