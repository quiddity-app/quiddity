//! The device's key, in a file of its own in the device's configuration
//! directory.
//!
//! It is not in a graph's file, so copying a graph doesn't copy the device,
//! and one key serves every graph the device holds (each derives its own from
//! it, see `docs/Identity.md`). The file holds the key as `qdk_…` text, so a
//! scanner can find one that is committed by mistake, and is readable by its
//! owner only.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use quiddity_identity::DeviceKey;
use tracing::{info, warn};

use crate::StoreError;

const FILE: &str = "device.key";

/// Makes the names a file is built under unique within the process; the
/// process ID covers other processes.
static STAGING: AtomicU64 = AtomicU64::new(0);

/// Where a device keeps its key.
#[derive(Debug, Clone)]
pub struct Keyring {
    dir: PathBuf,
}

impl Keyring {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The directory a device keeps its key in unless told otherwise: the
    /// platform's, for a program of this name. `None` if the environment
    /// doesn't say where.
    pub fn default_dir() -> Option<PathBuf> {
        config_dir(Platform::current(), |name| std::env::var_os(name))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self) -> PathBuf {
        self.dir.join(FILE)
    }

    /// The device's key, made on first use.
    ///
    /// A key that is there but can't be read is an error, never replaced: a
    /// new one would be a different device, and the old one is not ours to
    /// destroy. Two processes asking at once agree on one key.
    pub fn key(&self) -> Result<DeviceKey, StoreError> {
        match self.read() {
            Ok(Some(key)) => return Ok(key),
            Ok(None) => {}
            Err(e) => return Err(e),
        }
        info!(path = %self.path().display(), "making this device's key");
        self.create()?;
        self.read()?
            .ok_or_else(|| self.unreadable("it was made and is gone".into()))
    }

    fn read(&self) -> Result<Option<DeviceKey>, StoreError> {
        let text = match fs::read_to_string(self.path()) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(self.unreadable(e.to_string())),
        };
        #[cfg(unix)]
        if let Ok(meta) = fs::metadata(self.path())
            && readable_by_others(std::os::unix::fs::PermissionsExt::mode(&meta.permissions()))
        {
            warn!(path = %self.path().display(), "this device's key can be read by others: restrict it to its owner");
        }
        DeviceKey::from_token(text.trim())
            .map(Some)
            .map_err(|e| self.unreadable(e.to_string()))
    }

    /// Makes the file if there is none. It is built under another name, with
    /// its owner as its only reader from the start, and linked into place, so
    /// that it is never seen half written and the first to link wins.
    fn create(&self) -> Result<(), StoreError> {
        self.create_linking(|staged, key| fs::hard_link(staged, key))
    }

    /// [`Keyring::create`], with the step that puts the file in place given, so
    /// that a test can make it fail.
    fn create_linking(
        &self,
        link: impl FnOnce(&Path, &Path) -> io::Result<()>,
    ) -> Result<(), StoreError> {
        // A directory made for the key is private. One that is there already
        // was made by someone who may mean it to be shared, and is left as it
        // is: the key's own mode is what protects it.
        let made = !self.dir.exists();
        fs::create_dir_all(&self.dir)?;
        #[cfg(unix)]
        if made {
            fs::set_permissions(
                &self.dir,
                std::os::unix::fs::PermissionsExt::from_mode(0o700),
            )?;
        }
        #[cfg(not(unix))]
        let _ = made;
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
        let key = DeviceKey::from_bytes(bytes);
        bytes.fill(0);

        let n = STAGING.fetch_add(1, Ordering::Relaxed);
        let staged = self
            .dir
            .join(format!("{FILE}.{}.{n}.new", std::process::id()));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&staged)?;
        let written = writeln!(file, "{}", &**key.to_token()).and_then(|()| file.sync_all());
        let linked = written.and_then(|()| link(&staged, &self.path()));
        let _ = fs::remove_file(&staged);
        match linked {
            Ok(()) => {
                #[cfg(unix)]
                fs::File::open(&self.dir)?.sync_all()?;
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn unreadable(&self, why: String) -> StoreError {
        StoreError::DeviceKey(self.path(), why)
    }
}

/// Whether a file with this mode can be read or written by anyone but its
/// owner.
#[cfg(unix)]
fn readable_by_others(mode: u32) -> bool {
    mode & 0o077 != 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Platform {
    Unix,
    Mac,
    Windows,
}

impl Platform {
    fn current() -> Self {
        if cfg!(windows) {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::Mac
        } else {
            Platform::Unix
        }
    }
}

/// Where a program called `quiddity` keeps its configuration on `platform`,
/// given the environment.
pub(crate) fn config_dir(
    platform: Platform,
    var: impl Fn(&str) -> Option<OsString>,
) -> Option<PathBuf> {
    // An empty or relative setting is no setting: it would put the key
    // wherever the program happened to be run.
    let absolute = |name: &str| var(name).map(PathBuf::from).filter(|p| p.is_absolute());
    let base = match platform {
        Platform::Windows => absolute("APPDATA")?,
        Platform::Mac => absolute("HOME")?.join("Library/Application Support"),
        Platform::Unix => match absolute("XDG_CONFIG_HOME") {
            Some(dir) => dir,
            None => absolute("HOME")?.join(".config"),
        },
    };
    Some(base.join("quiddity"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn the_configuration_directory_follows_the_platform() {
        let dir = |platform, pairs: &[(&str, &str)]| config_dir(platform, env(pairs));
        assert_eq!(
            dir(Platform::Unix, &[("HOME", "/home/a")]),
            Some("/home/a/.config/quiddity".into())
        );
        assert_eq!(
            dir(
                Platform::Unix,
                &[("HOME", "/home/a"), ("XDG_CONFIG_HOME", "/x")]
            ),
            Some("/x/quiddity".into())
        );
        assert_eq!(
            dir(
                Platform::Mac,
                &[("HOME", "/Users/a"), ("XDG_CONFIG_HOME", "/x")]
            ),
            Some("/Users/a/Library/Application Support/quiddity".into())
        );
        assert_eq!(
            dir(
                Platform::Windows,
                &[("APPDATA", "/Users/a/AppData"), ("HOME", "/h")]
            ),
            Some("/Users/a/AppData/quiddity".into())
        );
    }

    #[test]
    fn an_environment_that_does_not_say_gives_no_directory() {
        let dir = |platform, pairs: &[(&str, &str)]| config_dir(platform, env(pairs));
        assert_eq!(dir(Platform::Unix, &[]), None);
        assert_eq!(dir(Platform::Mac, &[]), None);
        assert_eq!(dir(Platform::Windows, &[("HOME", "/h")]), None);
        // Empty and relative settings are not settings.
        assert_eq!(dir(Platform::Unix, &[("HOME", "")]), None);
        assert_eq!(dir(Platform::Unix, &[("HOME", "home")]), None);
        assert_eq!(
            dir(
                Platform::Unix,
                &[("HOME", "/h"), ("XDG_CONFIG_HOME", "rel")]
            ),
            Some("/h/.config/quiddity".into())
        );
    }

    #[test]
    fn a_key_is_made_once_and_is_the_same_each_time() {
        let tmp = tempfile::tempdir().unwrap();
        let keyring = Keyring::new(tmp.path().join("config"));
        let first = keyring.key().unwrap().to_token();
        assert_eq!(keyring.key().unwrap().to_token(), first);
        assert_eq!(Keyring::new(keyring.dir()).key().unwrap().to_token(), first);
        // Another device's directory has another key.
        let other = Keyring::new(tmp.path().join("other"));
        assert_ne!(other.key().unwrap().to_token(), first);
    }

    #[test]
    fn the_file_holds_the_token_for_a_scanner_and_only_its_owner_can_read_it() {
        let tmp = tempfile::tempdir().unwrap();
        let keyring = Keyring::new(tmp.path());
        let token = keyring.key().unwrap().to_token();
        let text = fs::read_to_string(tmp.path().join("device.key")).unwrap();
        assert_eq!(text, format!("{}\n", &**token));
        assert!(text.starts_with("qdk_"));
        #[cfg(unix)]
        {
            let mode = |p: &Path| {
                std::os::unix::fs::PermissionsExt::mode(&fs::metadata(p).unwrap().permissions())
                    & 0o777
            };
            assert_eq!(mode(&tmp.path().join("device.key")), 0o600);
        }
        // Nothing is left beside it.
        let names: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["device.key"]);
    }

    #[cfg(unix)]
    #[test]
    fn the_directory_made_for_it_is_private() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("a/b");
        Keyring::new(&dir).key().unwrap();
        let mode =
            std::os::unix::fs::PermissionsExt::mode(&fs::metadata(&dir).unwrap().permissions());
        assert_eq!(mode & 0o777, 0o700);
    }

    /// Whatever is there that isn't a key is reported and left alone: a new
    /// key would be a different device.
    #[test]
    fn a_key_that_cannot_be_read_is_reported_not_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("device.key");
        let keyring = Keyring::new(tmp.path());
        let good = keyring.key().unwrap().to_token();
        for damaged in [
            String::new(),
            "not a key\n".to_owned(),
            good[..good.len() - 1].to_owned(),
            good.replacen('q', "x", 1),
        ] {
            fs::write(&path, &damaged).unwrap();
            let error = keyring.key().unwrap_err();
            assert!(matches!(error, StoreError::DeviceKey(..)), "{error:?}");
            assert_eq!(fs::read_to_string(&path).unwrap(), damaged, "left alone");
        }
        // The message says where, and never what is in the file.
        fs::write(&path, &good[..good.len() - 1]).unwrap();
        let message = keyring.key().unwrap_err().to_string();
        assert!(message.contains("device.key"), "{message}");
        assert!(!message.contains(&good[4..20]), "{message}");
    }

    #[test]
    fn processes_starting_together_agree_on_a_key() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("config");
        let tokens: Vec<_> = std::thread::scope(|s| {
            (0..8)
                .map(|_| s.spawn(|| Keyring::new(&dir).key().unwrap().to_token().to_string()))
                .collect::<Vec<_>>()
                .into_iter()
                .map(|t| t.join().unwrap())
                .collect()
        });
        assert!(tokens.windows(2).all(|w| w[0] == w[1]), "{tokens:?}");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "no staging left");
    }

    #[test]
    fn the_default_directory_is_the_platforms_for_the_environment() {
        assert_eq!(
            Keyring::default_dir(),
            config_dir(Platform::current(), |name| std::env::var_os(name))
        );
        if let Some(dir) = Keyring::default_dir() {
            assert!(dir.is_absolute() && dir.ends_with("quiddity"), "{dir:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn only_the_owner_may_read_the_key() {
        assert!(!readable_by_others(0o600));
        assert!(!readable_by_others(0o400));
        assert!(!readable_by_others(0o700));
        for mode in [
            0o640, 0o604, 0o660, 0o606, 0o644, 0o666, 0o610, 0o601, 0o670,
        ] {
            assert!(readable_by_others(mode), "{mode:o}");
        }
    }

    /// What can't be read for another reason than being absent is reported,
    /// not taken for absent and made again.
    #[test]
    fn a_key_that_is_there_and_unreadable_is_not_taken_for_absent() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("device.key")).unwrap();
        let message = Keyring::new(tmp.path()).key().unwrap_err().to_string();
        assert!(message.contains("device.key"), "{message}");
        assert!(!message.contains("gone"), "it was not made: {message}");
        assert!(tmp.path().join("device.key").is_dir(), "left alone");
    }

    /// A directory that was there is left as it was, whoever it is shared with.
    #[cfg(unix)]
    #[test]
    fn a_directory_that_was_there_keeps_its_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("shared");
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        Keyring::new(&dir).key().unwrap();
        let mode =
            std::os::unix::fs::PermissionsExt::mode(&fs::metadata(&dir).unwrap().permissions());
        assert_eq!(mode & 0o777, 0o755);
    }

    /// Another process that got there first is not a failure; anything else
    /// is, and leaves nothing behind.
    #[test]
    fn only_losing_the_race_is_not_a_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let fails = |kind: io::ErrorKind| {
            let keyring = Keyring::new(tmp.path().join(format!("{kind:?}")));
            let result = keyring.create_linking(|_, _| Err(io::Error::from(kind)));
            (keyring, result)
        };

        let (keyring, lost) = fails(io::ErrorKind::AlreadyExists);
        assert!(lost.is_ok());
        assert_eq!(
            fs::read_dir(keyring.dir()).unwrap().count(),
            0,
            "no staging left"
        );

        let (keyring, denied) = fails(io::ErrorKind::PermissionDenied);
        match denied.unwrap_err() {
            StoreError::Io(e) => assert_eq!(e.kind(), io::ErrorKind::PermissionDenied),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            fs::read_dir(keyring.dir()).unwrap().count(),
            0,
            "no staging left"
        );
        assert!(!keyring.dir().join("device.key").exists());
    }
}
