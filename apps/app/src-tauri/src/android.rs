//! What the Android plugin (`plugins/background-sync`) calls in this library,
//! through JNI. **A thin shim**: it turns strings and numbers into what
//! `background.rs` takes and back, and the logic is there, where it is tested.
//! It needs a JVM to run, so nothing here is, but it is compiled by the tests
//! on any platform (`lib.rs`), so that a change that breaks it fails there and
//! not first in the release build.
//!
//! The names are the plugin's package and class (`uk.co.aylett.quiddity.sync`,
//! `Native`): change one and the other, or the call fails to link at run time.
//!
//! A run in a process that has no window installs no JNI context for iroh to
//! read the system's DNS from: iroh falls back to its own nameservers (its
//! documentation on `Endpoint` says so). Installing one here would instead
//! panic the app if its window then started in the same process, since the
//! window's glue installs the same.

#![allow(unsafe_code, reason = "JNI exports are `no_mangle`, which is unsafe")]

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::time::Duration;

use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jint};
use tauri::Manager;

use crate::background;

/// Warns if Tauri keeps the app's data and configuration anywhere but where a
/// background run, which has no Tauri to ask, looks (`background::android_dir`):
/// such a run would find no graph, and say so, and never make one.
#[cfg_attr(
    not(target_os = "android"),
    allow(
        dead_code,
        reason = "only the tests compile this elsewhere, to check it builds"
    )
)]
pub fn check_dirs(app: &tauri::App) {
    let paths = app.path();
    let (Ok(base), Ok(data), Ok(config)) = (
        paths.data_dir(),
        paths.app_data_dir(),
        paths.app_config_dir(),
    ) else {
        return;
    };
    let ours = background::android_dir(&base, &app.config().identifier);
    if data != ours || config != ours {
        tracing::warn!(
            data = %data.display(),
            config = %config.display(),
            expected = %ours.display(),
            "Tauri keeps the app's files elsewhere than background sync looks"
        );
    }
}

fn string(env: &mut JNIEnv<'_>, from: &JString<'_>) -> Option<String> {
    env.get_string(from).ok().map(String::from)
}

/// Syncs once and says how it went (`Outcome::code`), blocking until it has.
/// `blobs` is whether to fetch files as well, which the plugin turns off on a
/// metered network.
#[unsafe(no_mangle)]
pub extern "system" fn Java_uk_co_aylett_quiddity_sync_Native_syncOnce<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    data_dir: JString<'local>,
    identifier: JString<'local>,
    timeout_secs: jint,
    blobs: jboolean,
) -> jint {
    let (Some(data_dir), Some(identifier)) =
        (string(&mut env, &data_dir), string(&mut env, &identifier))
    else {
        return background::FAILED;
    };
    let dir = background::android_dir(&PathBuf::from(data_dir), &identifier);
    let timeout = Duration::from_secs(u64::try_from(timeout_secs).unwrap_or(0));
    // A panic must not unwind into the JVM.
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        tauri::async_runtime::block_on(background::sync_in(&dir, &dir, timeout, blobs != 0))
    }));
    match outcome {
        Ok(Ok(outcome)) => outcome.code(),
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "background sync failed");
            background::FAILED
        }
        Err(_) => background::FAILED,
    }
}

/// Whether the live session has a sync the user shouldn't interrupt
/// (`background::outstanding`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_uk_co_aylett_quiddity_sync_Native_outstanding<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jboolean {
    jboolean::from(catch_unwind(background::outstanding).unwrap_or(false))
}
