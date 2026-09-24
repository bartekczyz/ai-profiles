//! Copying a session's files into another config dir without losing any.
//!
//! A copy is built under a temporary name beside where it goes and renamed
//! into place, so Claude Code never reads half a file. Whatever it replaces is
//! first moved aside into a backup folder, never removed. Files and folders
//! keep their modification time, which Claude Code sorts sessions by.

use std::fs::{self, File};
use std::io::{self, BufReader, Read};
use std::path::Path;

use super::archive_store::move_all;
use crate::error::AppResult;
use crate::sessions::fs_ops::{copy_tree, occupied, remove_temp, stage};
use crate::sessions::move_plan::ItemAction;

/// What a move would do with `from` to put it at `to`. Files are taken to be
/// the same only with the same length, modification time and bytes; see
/// [`identical`].
pub fn compare(from: &Path, to: &Path) -> AppResult<ItemAction> {
    if !occupied(to) {
        return Ok(ItemAction::Copy);
    }
    if identical(from, to)? {
        return Ok(ItemAction::Same);
    }
    Ok(ItemAction::Replace)
}

/// Put a copy of `from`, a file or folder, at `relative` under `destination`.
/// What is there already is moved to `relative` under `backup` first, and put
/// back if the copy can't be renamed into place. The copy is built under a
/// temporary name, removed again when that fails. Returns whether something
/// was there, and so is in `backup` now.
pub fn place(from: &Path, destination: &Path, relative: &Path, backup: &Path) -> AppResult<bool> {
    let target = destination.join(relative);
    let temp = stage(&target, |temp| copy_tree(from, temp))?;
    finish(&temp, destination, relative, backup)
}

/// Replace the file at `relative` under `destination` with `contents`, moving
/// the one there to `relative` under `backup` first. Returns whether one was
/// there.
pub fn write_replacing(
    contents: &str,
    destination: &Path,
    relative: &Path,
    backup: &Path,
) -> AppResult<bool> {
    let target = destination.join(relative);
    let temp = stage(&target, |temp| fs::write(temp, contents))?;
    finish(&temp, destination, relative, backup)
}

/// Rename the staged `temp` into `relative` under `destination`, moving what
/// is there to `relative` under `backup` first and back if the rename fails.
/// Returns whether something was there.
fn finish(temp: &Path, destination: &Path, relative: &Path, backup: &Path) -> AppResult<bool> {
    let target = destination.join(relative);
    let items = [relative.to_path_buf()];
    let backed_up = occupied(&target);
    let rename = &mut |from: &Path, to: &Path| fs::rename(from, to);
    if backed_up {
        if let Err(failed) = move_all(&items, destination, backup, rename) {
            remove_temp(temp);
            return Err(failed.error);
        }
    }
    if let Err(error) = fs::rename(temp, &target) {
        remove_temp(temp);
        if backed_up {
            if let Err(failed) = move_all(&items, backup, destination, rename) {
                return Err(failed
                    .stranded_error(&format!("so it stays backed up in {}", backup.display())));
            }
        }
        return Err(error.into());
    }
    Ok(backed_up)
}

/// Whether `left` and `right` hold the same: files of the same length and
/// modification time, then the same bytes, links to the same place, or
/// folders whose entries are all the same. A copy keeps its file's date, so
/// files dated apart are taken to differ without reading either, which
/// spares reading two copies of a long transcript whole.
fn identical(left: &Path, right: &Path) -> AppResult<bool> {
    let (left_metadata, right_metadata) =
        (fs::symlink_metadata(left)?, fs::symlink_metadata(right)?);
    let (left_kind, right_kind) = (left_metadata.file_type(), right_metadata.file_type());
    if left_kind.is_symlink() || right_kind.is_symlink() {
        return Ok(left_kind.is_symlink()
            && right_kind.is_symlink()
            && fs::read_link(left)? == fs::read_link(right)?);
    }
    if left_kind.is_file() && right_kind.is_file() {
        return Ok(left_metadata.len() == right_metadata.len()
            && left_metadata.modified()? == right_metadata.modified()?
            && same_bytes(left, right)?);
    }
    if !(left_kind.is_dir() && right_kind.is_dir()) {
        return Ok(false);
    }
    let (left_names, right_names) = (entry_names(left)?, entry_names(right)?);
    if left_names != right_names {
        return Ok(false);
    }
    for name in left_names {
        if !identical(&left.join(&name), &right.join(&name))? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether the files `left` and `right` hold the same bytes, read side by
/// side a block at a time.
pub fn same_bytes(left: &Path, right: &Path) -> io::Result<bool> {
    let (mut left, mut right) = (
        BufReader::new(File::open(left)?),
        BufReader::new(File::open(right)?),
    );
    let (mut left_block, mut right_block) = (vec![0_u8; 64 * 1024], vec![0_u8; 64 * 1024]);
    loop {
        let read = left.read(&mut left_block)?;
        if read == 0 {
            return Ok(right.read(&mut right_block)? == 0);
        }
        let mut filled = 0;
        while filled < read {
            let more = right.read(&mut right_block[filled..read])?;
            if more == 0 {
                return Ok(false);
            }
            filled += more;
        }
        if left_block[..read] != right_block[..read] {
            return Ok(false);
        }
    }
}

/// The names of the entries in `dir`, sorted.
fn entry_names(dir: &Path) -> io::Result<Vec<std::ffi::OsString>> {
    let mut names = fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<io::Result<Vec<_>>>()?;
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
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

    /// Date the file at `path` `modified`.
    fn date(path: &Path, modified: SystemTime) {
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
    }

    /// Write `contents` to `path`, making its folder, dated `modified`.
    fn write_dated(path: &Path, contents: &str, modified: SystemTime) {
        write(path, contents);
        date(path, modified);
    }

    #[test]
    fn an_item_is_copied_when_missing_left_when_the_same_and_replaced_when_not() {
        let root = tempdir().unwrap();
        let from = root.path().join("from");
        let to = root.path().join("to");
        let written = SystemTime::UNIX_EPOCH + Duration::from_secs(1_780_000_000);
        write(&from.join("a/one.txt"), "1");
        write_dated(&from.join("b/one.txt"), "1", written);
        write_dated(&to.join("b/one.txt"), "1", written);
        write_dated(&from.join("c/one.txt"), "1", written);
        write_dated(&to.join("c/one.txt"), "2", written);
        write_dated(&from.join("d/one.txt"), "1", written);
        write_dated(&to.join("d/one.txt"), "1", written);
        write(&to.join("d/two.txt"), "2");

        let actions: Vec<ItemAction> = ["a", "b", "c", "d"]
            .iter()
            .map(|name| compare(&from.join(name), &to.join(name)).unwrap())
            .collect();

        assert_eq!(
            actions,
            [
                ItemAction::Copy,
                ItemAction::Same,
                ItemAction::Replace,
                ItemAction::Replace
            ]
        );
    }

    #[test]
    fn a_file_dated_differently_is_different_without_reading_it() {
        let root = tempdir().unwrap();
        let from = root.path().join("from.jsonl");
        let to = root.path().join("to.jsonl");
        let written = SystemTime::UNIX_EPOCH + Duration::from_secs(1_780_000_000);
        write_dated(&from, "same", written);
        write_dated(&to, "same", written + Duration::from_secs(1));

        assert_eq!(compare(&from, &to).unwrap(), ItemAction::Replace);

        date(&to, written);

        assert_eq!(compare(&from, &to).unwrap(), ItemAction::Same);
    }

    #[test]
    fn a_placed_folder_keeps_its_date() {
        let root = tempdir().unwrap();
        let from = root.path().join("from/s");
        let destination = root.path().join("to");
        let written = SystemTime::UNIX_EPOCH + Duration::from_secs(1_780_000_000);
        write_dated(&from.join("subagents/agent-1.jsonl"), "new", written);
        for folder in [from.join("subagents"), from.clone()] {
            File::open(&folder).unwrap().set_modified(written).unwrap();
        }

        place(
            &from,
            &destination,
            Path::new("projects/s"),
            &root.path().join("backup"),
        )
        .unwrap();

        let placed = destination.join("projects/s");
        for folder in [placed.join("subagents"), placed] {
            assert_eq!(fs::metadata(&folder).unwrap().modified().unwrap(), written);
        }
    }

    #[test]
    fn a_placed_copy_keeps_its_dates_and_what_it_replaced_is_backed_up() {
        let root = tempdir().unwrap();
        let from = root.path().join("from/s");
        let destination = root.path().join("to");
        let backup = root.path().join("backup");
        let written = SystemTime::UNIX_EPOCH + Duration::from_secs(1_780_000_000);
        write(&from.join("subagents/agent-1.jsonl"), "new");
        File::options()
            .write(true)
            .open(from.join("subagents/agent-1.jsonl"))
            .unwrap()
            .set_modified(written)
            .unwrap();
        write(&destination.join("projects/s/old.jsonl"), "old");

        place(&from, &destination, Path::new("projects/s"), &backup).unwrap();

        let copied = destination.join("projects/s/subagents/agent-1.jsonl");
        assert_eq!(fs::read_to_string(&copied).unwrap(), "new");
        assert_eq!(fs::metadata(&copied).unwrap().modified().unwrap(), written);
        assert!(!destination.join("projects/s/old.jsonl").exists());
        assert_eq!(
            fs::read_to_string(backup.join("projects/s/old.jsonl")).unwrap(),
            "old"
        );
        assert_eq!(names(&destination.join("projects")), ["s"]);
    }

    #[test]
    fn a_copy_that_fails_leaves_the_destination_as_it_was() {
        let root = tempdir().unwrap();
        let from = root.path().join("from.jsonl");
        let destination = root.path().join("to");
        let backup = root.path().join("backup");
        write(&from, "new");
        write(&destination.join("projects/s.jsonl"), "old");
        let projects = destination.join("projects");
        fs::set_permissions(&projects, fs::Permissions::from_mode(0o555)).unwrap();

        let placed = place(&from, &destination, Path::new("projects/s.jsonl"), &backup);

        fs::set_permissions(&projects, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(placed.is_err());
        assert_eq!(names(&projects), ["s.jsonl"]);
        assert_eq!(fs::read_to_string(projects.join("s.jsonl")).unwrap(), "old");
        assert!(!backup.exists());
    }

    #[test]
    fn a_replaced_file_is_backed_up_before_it_is_rewritten() {
        let root = tempdir().unwrap();
        let destination = root.path().join("to");
        let backup = root.path().join("backup");
        write(&destination.join("memory/MEMORY.md"), "- a\n");

        write_replacing(
            "- a\n- b\n",
            &destination,
            Path::new("memory/MEMORY.md"),
            &backup,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(destination.join("memory/MEMORY.md")).unwrap(),
            "- a\n- b\n"
        );
        assert_eq!(
            fs::read_to_string(backup.join("memory/MEMORY.md")).unwrap(),
            "- a\n"
        );
        assert_eq!(names(&destination.join("memory")), ["MEMORY.md"]);
    }
}
