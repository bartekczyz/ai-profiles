//! Carrying out a planned move, and taking it back when it fails part way.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::{refuse_running, Item, MoveReport, Prepared, RecordWrite};
use crate::error::{AppError, AppResult};
use crate::sessions::actions::SessionAction;
use crate::sessions::claude::archive;
use crate::sessions::claude::archive_store::{move_all, occupied, replaced_dir};
use crate::sessions::claude::copy::{place, ItemAction};
use crate::sessions::claude::desktop::{
    build_destination_record, deleted_in, list_as_active, write_destination_record, ARCHIVED_INDEX,
};
use crate::sessions::claude::memory::merge_memory;
use crate::sessions::Home;

/// Where, in a move's backup folder, a desktop record it replaced goes, with
/// the archived index it rewrote.
const RECORDS_BACKUP: &str = "desktop-records";

/// Where, in a move's backup folder, what a move that failed had put at the
/// destination is set aside, laid out like the backup folder.
const UNDONE_DIR: &str = "undone";

/// Something a move put at the destination, to take back should a later step
/// fail.
#[derive(Debug)]
struct Change {
    /// The folder it is in.
    dir: PathBuf,
    /// It, relative to `dir`.
    relative: PathBuf,
    /// The folder holding what it replaced, at `relative`, if it replaced
    /// anything.
    backup_dir: Option<PathBuf>,
    /// The folder it is set aside into, at `relative`, when taken back.
    undone_dir: PathBuf,
}

/// Carry out `prepared`, a move [`plan`](super::plan) let through, at `at`: copy the
/// files, merge the memory, copy the transcripts, write the destination's
/// desktop record, then archive the session at the source. What is replaced
/// is backed up in the destination's config dir first (see
/// [`replaced_dir`]).
///
/// A step that fails stops the move there, and what it copied and wrote at
/// the destination is taken back: set aside into the backup folder, with
/// what it had replaced put back, never replacing anything. The memory it
/// merged stays, as that only adds what the destination lacked. The source is
/// untouched until the last step. The error says what became of it all, and
/// keeps the kind of the failure.
///
/// The destination's desktop app must not run while anything of its home is
/// written, so one that started again since the check is looked for right
/// before the first write, and the move refused if it runs.
pub fn execute(prepared: Prepared, at: DateTime<Utc>) -> AppResult<MoveReport> {
    let backup = replaced_dir(&prepared.destination.config_dir, &prepared.session_id, at)?;
    if prepared.writes_destination {
        refuse_running(&prepared.destination)?;
    }
    let mut changes = Vec::new();
    carry_out(prepared, &backup, &mut changes).map_err(|error| take_back(error, changes, &backup))
}

/// `error`, which stopped a move part way, once the `changes` the move made at
/// the destination are taken back, saying so and where what it set aside is
/// kept: `backup`.
fn take_back(error: AppError, changes: Vec<Change>, backup: &Path) -> AppError {
    let kept = backup.display().to_string();
    if changes.is_empty() {
        if !backup.exists() {
            return error;
        }
        return error.map_message(|message| {
            format!("{message}. What the move replaced is backed up in {kept}")
        });
    }
    let failures = undo(changes);
    error.map_message(|message| {
        if failures.is_empty() {
            return format!(
                "{message}. The move was taken back; what it set aside is kept in {kept}"
            );
        }
        format!(
            "{message}. Taking the move back failed too: {}. What it set aside is kept in {kept}",
            failures.join("; ")
        )
    })
}

/// Take back `changes`, the last first: set each aside into its undone
/// folder, then put what it replaced back. Nothing is ever replaced on the
/// way. Returns what couldn't be taken back, naming what stays where.
fn undo(changes: Vec<Change>) -> Vec<String> {
    let rename = &mut |from: &Path, to: &Path| fs::rename(from, to);
    let mut failures = Vec::new();
    for change in changes.into_iter().rev() {
        let items = std::slice::from_ref(&change.relative);
        // A record that failed to be written is not there to set aside.
        if occupied(&change.dir.join(&change.relative)) {
            if let Err(failed) = move_all(items, &change.dir, &change.undone_dir, rename) {
                failures.push(format!(
                    "{} couldn't be set aside ({}), so it stays",
                    change.dir.join(&change.relative).display(),
                    failed.error.message()
                ));
                continue;
            }
        }
        let Some(backup_dir) = &change.backup_dir else {
            continue;
        };
        if let Err(failed) = move_all(items, backup_dir, &change.dir, rename) {
            failures.push(format!(
                "{} couldn't be put back ({}), so it stays in {}",
                change.relative.display(),
                failed.error.message(),
                backup_dir.display()
            ));
        }
    }
    failures
}

/// [`execute()`] `prepared`, backing up what it replaces into `backup` and
/// logging what it puts at the destination in `changes`.
fn carry_out(
    prepared: Prepared,
    backup: &Path,
    changes: &mut Vec<Change>,
) -> AppResult<MoveReport> {
    let Prepared {
        source,
        destination,
        items,
        memory,
        record,
        archive,
        ..
    } = prepared;
    let (transcripts, others): (Vec<&Item>, Vec<&Item>) =
        items.iter().partition(|item| item.used_at.is_some());
    for item in others {
        copy_item(item, &destination, backup, changes)?;
    }
    let mut memory_conflicts = Vec::new();
    for merge in &memory {
        memory_conflicts.extend(merge_memory(
            &merge.from,
            &destination.config_dir.join(&merge.relative),
            &backup.join(&merge.relative),
        )?);
    }
    for item in transcripts {
        copy_item(item, &destination, backup, changes)?;
    }
    if let Some(write) = record {
        write_record(&destination, write, backup, changes)?;
    }
    archive::apply(&source, archive, SessionAction::Archive)?;
    Ok(MoveReport { memory_conflicts })
}

/// Copy `item` into `destination`'s config dir, backing up what it replaces
/// into `backup`, and log it in `changes`. One the destination has the same
/// of is left alone.
fn copy_item(
    item: &Item,
    destination: &Home,
    backup: &Path,
    changes: &mut Vec<Change>,
) -> AppResult<()> {
    if item.action == ItemAction::Same {
        return Ok(());
    }
    let replaced = place(&item.from, &destination.config_dir, &item.relative, backup)?;
    changes.push(Change {
        dir: destination.config_dir.clone(),
        relative: item.relative.clone(),
        backup_dir: replaced.then(|| backup.to_path_buf()),
        undone_dir: backup.join(UNDONE_DIR),
    });
    Ok(())
}

/// Write `destination`'s desktop record of the moved session, and take it
/// out of the archived index there, logging both in `changes`. Its desktop
/// app writes its records back from memory, so one that started again since
/// the check is looked for right before, and the write refused if it runs. A
/// record already there under the same id (archived there) is backed up into
/// `backup` first, as is the index before it is rewritten; an id the app
/// marks deleted there is swapped for a new one, as the app would never show
/// it.
fn write_record(
    destination: &Home,
    write: RecordWrite,
    backup: &Path,
    changes: &mut Vec<Change>,
) -> AppResult<()> {
    refuse_running(destination)?;
    let mut record =
        build_destination_record(write.source.as_ref(), &write.displayed, &write.priors)?;
    let local_id = record["sessionId"].as_str().unwrap_or_default().to_string();
    if deleted_in(&write.account_dir, &local_id) {
        record["sessionId"] = Value::String(format!("local_{}", uuid::Uuid::new_v4()));
    }
    let name = PathBuf::from(format!(
        "{}.json",
        record["sessionId"].as_str().unwrap_or_default()
    ));
    let records_backup = backup.join(RECORDS_BACKUP);
    let undone_dir = backup.join(UNDONE_DIR).join(RECORDS_BACKUP);
    let replaced = occupied(&write.account_dir.join(&name));
    if replaced {
        move_all(
            std::slice::from_ref(&name),
            &write.account_dir,
            &records_backup,
            &mut |from, to| fs::rename(from, to),
        )
        .map_err(|failed| failed.error)?;
    }
    // Logged before the write, so a record set aside is put back even when
    // the new one can't be written.
    changes.push(Change {
        dir: write.account_dir.clone(),
        relative: name,
        backup_dir: replaced.then(|| records_backup.clone()),
        undone_dir: undone_dir.clone(),
    });
    let path = write_destination_record(&write.account_dir, &record)?;
    let local_id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    if list_as_active(&write.account_dir, local_id, &records_backup)? {
        changes.push(Change {
            dir: write.account_dir,
            relative: PathBuf::from(ARCHIVED_INDEX),
            backup_dir: Some(records_backup),
            undone_dir,
        });
    }
    Ok(())
}
