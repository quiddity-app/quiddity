type:: crate

- Tauri's official plugin for updating an app in place: it reads a JSON feed of releases, downloads the bundle for the platform, checks its minisign signature against a public key, and installs it. https://v2.tauri.app/plugin/updater/
- [[The shell]] uses its Rust API on the desktop only, behind two commands of its own, rather than giving the UI the plugin's JavaScript commands. [[Updates]]
