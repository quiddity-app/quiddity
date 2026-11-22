//! Putting a file made under another name in its place, unless one is there.

use std::fs;
use std::io;
use std::path::Path;

/// Moves `staged` to `target` only if nothing is at `target`, failing with
/// `AlreadyExists` if there is. A hard link does it atomically, so a file is
/// never seen half written and the first of two racing processes wins.
///
/// Android's `SELinux` policy refuses `link` to apps, even inside their own data
/// directory, so a refusal falls back to checking and then renaming. That
/// leaves a window in which two processes could both pass the check, which an
/// app alone in its data directory doesn't have.
pub(crate) fn link_new(staged: &Path, target: &Path) -> io::Result<()> {
    link_new_with(staged, target, |from, to| fs::hard_link(from, to))
}

/// [`link_new`], with the hard link given, so that a test can refuse it.
fn link_new_with(
    staged: &Path,
    target: &Path,
    link: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    match link(staged, target) {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            // `symlink_metadata`, so that a dangling link counts as there.
            if target.symlink_metadata().is_ok() {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            fs::rename(staged, target)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(_: &Path, _: &Path) -> io::Result<()> {
        Err(io::ErrorKind::PermissionDenied.into())
    }

    #[test]
    fn a_refused_link_still_puts_the_file_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        let (staged, target) = (tmp.path().join("staged"), tmp.path().join("target"));
        fs::write(&staged, "mine").unwrap();
        link_new_with(&staged, &target, refused).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "mine");
        assert!(!staged.exists());
    }

    #[test]
    fn a_refused_link_leaves_what_is_there() {
        let tmp = tempfile::tempdir().unwrap();
        let (staged, target) = (tmp.path().join("staged"), tmp.path().join("target"));
        fs::write(&staged, "mine").unwrap();
        fs::write(&target, "theirs").unwrap();
        let e = link_new_with(&staged, &target, refused).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&target).unwrap(), "theirs");
    }

    #[test]
    fn other_failures_are_not_worked_around() {
        let tmp = tempfile::tempdir().unwrap();
        let (staged, target) = (tmp.path().join("staged"), tmp.path().join("target"));
        fs::write(&staged, "mine").unwrap();
        let e = link_new_with(&staged, &target, |_, _| Err(io::ErrorKind::NotFound.into()))
            .unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::NotFound);
        assert!(!target.exists());
    }

    #[test]
    fn a_real_link_is_used_when_it_works() {
        let tmp = tempfile::tempdir().unwrap();
        let (staged, target) = (tmp.path().join("staged"), tmp.path().join("target"));
        fs::write(&staged, "mine").unwrap();
        link_new(&staged, &target).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "mine");
        assert_eq!(
            link_new(&staged, &target).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
    }
}
