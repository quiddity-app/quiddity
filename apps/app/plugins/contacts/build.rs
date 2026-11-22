// The plugin has no commands for the page to call: the shell calls the
// Android side itself.
const COMMANDS: &[&str] = &[];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
