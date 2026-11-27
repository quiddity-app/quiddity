fn main() {
    // No commands: the plugin is Kotlin, started by the app, and the calls it
    // makes are into the app's library (`apps/app/src-tauri/src/android.rs`).
    tauri_plugin::Builder::new(&[])
        .android_path("android")
        .build();
}
