//! Background sync on Android. The plugin is its Kotlin (`android/`): this only
//! registers it, so that Tauri starts it with the activity and the Gradle
//! project is built in. Elsewhere it does nothing.
//!
//! Where the work is is `docs/Background sync.md`.

use tauri::Runtime;
use tauri::plugin::{Builder, TauriPlugin};

/// The Kotlin package, which is also what the JNI names in the app's
/// `android.rs` spell out.
#[cfg(target_os = "android")]
const PACKAGE: &str = "uk.co.aylett.quiddity.sync";

/// The plugin, for `tauri::Builder::plugin`.
#[must_use]
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("background-sync")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            _api.register_android_plugin(PACKAGE, "BackgroundSyncPlugin")?;
            Ok(())
        })
        .build()
}
