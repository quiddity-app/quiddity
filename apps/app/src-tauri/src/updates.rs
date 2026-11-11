//! Updates for the desktop app, from the releases `release.yml` publishes.
//!
//! The UI asks [`check_for_update`] and, if the user says yes, calls
//! [`install_update`], which downloads the update the check found, verifies
//! its signature against the public key in `tauri.conf.json`, installs it and
//! restarts. Nothing is installed without that call.
//!
//! Neither mobile platform lets an app replace itself, so there the commands
//! say there is nothing to install.

use serde::Serialize;

/// What the UI is told about an update that is there to install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Available {
    version: String,
    /// The release's notes, as its author wrote them.
    notes: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(desktop)]
    #[error("{0}")]
    Updater(#[from] tauri_plugin_updater::Error),
    #[error("no update has been found to install")]
    NothingFound,
    #[cfg(mobile)]
    #[error("this platform is updated some other way")]
    Unsupported,
}

impl Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

/// The update the last check found, for [`install_update`] to install.
#[cfg(desktop)]
#[derive(Default)]
pub struct Pending(std::sync::Mutex<Option<tauri_plugin_updater::Update>>);

/// Asks the release feed whether there is a newer version. A feed that can't
/// be reached, or has nothing yet (there is no release to look at until the
/// first), is an error the UI may ignore.
#[cfg(desktop)]
#[tauri::command]
pub async fn check_for_update(
    app: tauri::AppHandle,
    pending: tauri::State<'_, Pending>,
) -> Result<Option<Available>, Error> {
    use tauri_plugin_updater::UpdaterExt;

    let update = app.updater()?.check().await?;
    let available = update.as_ref().map(|update| Available {
        version: update.version.clone(),
        notes: update.body.clone(),
    });
    if let Some(available) = &available {
        tracing::info!(version = %available.version, "an update is available");
    }
    // Replaces what was found before, so a later check never leaves an older
    // update to be installed.
    *pending.0.lock().expect("update lock poisoned") = update;
    Ok(available)
}

/// Installs the update the last check found and restarts the app.
#[cfg(desktop)]
#[tauri::command]
pub async fn install_update(
    app: tauri::AppHandle,
    pending: tauri::State<'_, Pending>,
) -> Result<(), Error> {
    // Taken out, so the lock isn't held across the download.
    let update = pending
        .0
        .lock()
        .expect("update lock poisoned")
        .take()
        .ok_or(Error::NothingFound)?;
    update.download_and_install(|_, _| {}, || {}).await?;
    app.restart()
}

#[cfg(mobile)]
#[tauri::command]
pub async fn check_for_update() -> Result<Option<Available>, Error> {
    Ok(None)
}

#[cfg(mobile)]
#[tauri::command]
pub async fn install_update() -> Result<(), Error> {
    Err(Error::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The updater plugin refuses to start without a public key and
    /// endpoints, so a config that lost either would stop the app opening.
    #[cfg(desktop)]
    #[test]
    fn the_config_names_a_feed_the_plugin_accepts() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let updater = &config["plugins"]["updater"];
        assert!(updater["pubkey"].is_string(), "the plugin needs a pubkey");
        let endpoints = updater["endpoints"].as_array().expect("endpoints");
        assert!(!endpoints.is_empty());
        for endpoint in endpoints {
            let url = endpoint.as_str().expect("an endpoint is a string");
            assert!(url.starts_with("https://"), "{url} is not https");
        }
    }

    #[test]
    fn errors_reach_the_ui_as_words() {
        let json = serde_json::to_string(&Error::NothingFound).unwrap();
        assert_eq!(json, "\"no update has been found to install\"");
    }
}
