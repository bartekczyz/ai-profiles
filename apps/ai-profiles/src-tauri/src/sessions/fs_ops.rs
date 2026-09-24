//! File moves and copies that never replace or lose what is there.
//!
//! A copy is built under a temporary name beside where it goes and renamed
//! into place, so no app reads half a file, and keeps its modification time,
//! which the apps sort sessions by.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

/// Something is at `path`: a file, a folder or a link, even a dangling one.
pub(in crate::sessions) fn occupied(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Put a copy of the file `from` at `target`, which must be free: nothing
/// there is ever replaced, even something that shows up while copying. The
/// copy is built under a temporary name beside `target` and moved into place
/// with [`move_new`], keeping its modification time; the temporary copy is
/// removed again when that fails.
pub(in crate::sessions) fn place_new(from: &Path, target: &Path) -> AppResult<()> {
    let taken = || AppError::Validation(format!("{} is taken", target.display()));
    // Refusing up front saves copying a file only to throw it away.
    if occupied(target) {
        return Err(taken());
    }
    let temp = stage(target, |temp| copy_tree(from, temp))?;
    match move_new(&temp, target) {
        Ok(None) => Ok(()),
        // The copy is in place; the temporary name left beside it is ours,
        // and is cleared by the next copy there if not now.
        Ok(Some(left)) => {
            remove_temp(&left);
            Ok(())
        }
        Err(error) => {
            remove_temp(&temp);
            if error.kind() == io::ErrorKind::AlreadyExists {
                return Err(taken());
            }
            Err(error.into())
        }
    }
}

/// Move the file at `from` to `to`, in the same folder tree, only if `to` is
/// free. A rename would replace whatever is at `to`, so the file is linked
/// there instead, which fails with [`io::ErrorKind::AlreadyExists`] when
/// something is; only then is `from` unlinked. The file itself, its contents
/// and modification time included, is the same one throughout.
///
/// Once linked, the file is at `to` whatever happens next, so a `from` that
/// can't be unlinked is no failure: it is returned instead, for the caller to
/// say the file is still also there, as a second link to it.
pub(in crate::sessions) fn move_new(from: &Path, to: &Path) -> io::Result<Option<PathBuf>> {
    move_new_with(from, to, |from| fs::remove_file(from))
}

/// [`move_new`], unlinking `from` with `unlink`.
fn move_new_with(
    from: &Path,
    to: &Path,
    unlink: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<Option<PathBuf>> {
    fs::hard_link(from, to)?;
    match unlink(from) {
        Ok(()) => Ok(None),
        Err(_) => Ok(Some(from.to_path_buf())),
    }
}

/// Build what goes to `target` with `build`, under a temporary name beside
/// it, making its folder. Returns the temporary path. A leftover of an earlier
/// attempt is ours and is cleared first; a failed build is cleared too.
pub(in crate::sessions) fn stage(
    target: &Path,
    build: impl FnOnce(&Path) -> io::Result<()>,
) -> AppResult<PathBuf> {
    let name = target
        .file_name()
        .ok_or_else(|| AppError::Validation(format!("{} has no name", target.display())))?;
    let temp = target.with_file_name(format!(".{}.ai-profiles-tmp", name.to_string_lossy()));
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    remove_temp(&temp);
    if let Err(error) = build(&temp) {
        remove_temp(&temp);
        return Err(error.into());
    }
    Ok(temp)
}

/// Remove a temporary copy of ours at `temp`, if there is one.
pub(in crate::sessions) fn remove_temp(temp: &Path) {
    match fs::symlink_metadata(temp) {
        Ok(metadata) if metadata.is_dir() => {
            let _ = fs::remove_dir_all(temp);
        }
        Ok(_) => {
            let _ = fs::remove_file(temp);
        }
        Err(_) => {}
    }
}

/// Copy `from`, a file, folder or link, to `to`, which must not exist. Files
/// and folders keep their modification time, a folder's set once what it
/// holds is copied, as copying into it changes it.
pub(in crate::sessions) fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(from)?;
    let kind = metadata.file_type();
    if kind.is_symlink() {
        return std::os::unix::fs::symlink(fs::read_link(from)?, to);
    }
    if kind.is_dir() {
        fs::create_dir(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        return File::open(to)?.set_modified(metadata.modified()?);
    }
    fs::copy(from, to)?;
    File::options()
        .write(true)
        .open(to)?
        .set_modified(metadata.modified()?)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use tempfile::tempdir;

    use super::*;

    /// Write `contents` to `path`, making its folder.
    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    /// The names in `dir`, sorted, hidden ones included.
    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_new_copy_keeps_its_date_and_never_replaces_what_is_there() {
        let root = tempdir().unwrap();
        let from = root.path().join("from.jsonl");
        let target = root.path().join("to/archived/s.jsonl");
        let written = SystemTime::UNIX_EPOCH + Duration::from_secs(1_780_000_000);
        write(&from, "new");
        File::options()
            .write(true)
            .open(&from)
            .unwrap()
            .set_modified(written)
            .unwrap();

        place_new(&from, &target).unwrap();
        write(&from, "newer");
        let again = place_new(&from, &target);

        assert!(again.is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(fs::metadata(&target).unwrap().modified().unwrap(), written);
        assert_eq!(names(&root.path().join("to/archived")), ["s.jsonl"]);
    }

    #[test]
    fn moving_a_file_onto_one_that_appeared_meanwhile_replaces_nothing() {
        let root = tempdir().unwrap();
        let from = root.path().join(".s.jsonl.ai-profiles-tmp");
        let to = root.path().join("s.jsonl");
        write(&from, "new");
        write(&to, "theirs");

        let moved = move_new(&from, &to);

        assert_eq!(moved.unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&to).unwrap(), "theirs");
        assert_eq!(fs::read_to_string(&from).unwrap(), "new");
    }

    #[test]
    fn a_moved_file_that_cant_be_unlinked_is_placed_and_its_copy_named() {
        let root = tempdir().unwrap();
        let from = root.path().join("a/s.jsonl");
        let to = root.path().join("a/s.jsonl.failed");
        write(&from, "new");

        let moved = move_new_with(&from, &to, |_| Err(io::ErrorKind::PermissionDenied.into()));

        assert_eq!(moved.unwrap(), Some(from.clone()));
        assert_eq!(fs::read_to_string(&to).unwrap(), "new");
        assert!(from.exists());
    }

    #[test]
    fn a_moved_file_is_only_at_its_new_place() {
        let root = tempdir().unwrap();
        let from = root.path().join("a/s.jsonl");
        let to = root.path().join("a/s.jsonl.failed");
        write(&from, "new");

        move_new(&from, &to).unwrap();

        assert_eq!(names(&root.path().join("a")), ["s.jsonl.failed"]);
    }
}
