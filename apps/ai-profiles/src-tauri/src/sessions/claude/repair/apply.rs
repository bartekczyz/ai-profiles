//! Carrying out a checked repair, one session at a time, each whole or not at
//! all.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use super::{
    in_use, live_by_home, PreparedRepair, RepairReport, SessionRepair, SkippedSession,
    TranscriptMove,
};
use crate::error::{AppError, AppResult};
use crate::launch::process_list;
use crate::sessions::claude::archive_store::{
    archive_bundle, move_all, occupied, replaced_dir, restore_bundle, UNDONE_DIR,
};
use crate::sessions::claude::copy::{compare, move_new, place, ItemAction};
use crate::sessions::claude::live::registrations;
use crate::sessions::claude::memory::merge_memory;
use crate::sessions::claude::transfer::refuse_running;
use crate::sessions::instance::{desktop_pid, running_again};
use crate::sessions::Home;

/// How long a listing of the running processes is taken to stay current
/// when looking for the repaired home's desktop app between sessions: well
/// under the seconds that app takes to start.
const LISTING_FRESH_FOR: Duration = Duration::from_millis(500);

/// Carry out `prepared`, a repair [`check`](super::check) found, at `at`: for each session,
/// copy the plans its transcripts wrote, merge its projects' memory, then move
/// its transcripts, the one shown last. Between homes on one volume a move is
/// a rename; across volumes each transcript is copied, then archived where it
/// was (see [`archive_store`](crate::sessions::claude::archive_store)).
///
/// Each session moves whole or not at all: when one of its transcripts can't
/// be moved, the ones that had are put back, and it is reported with the
/// rest skipped, while the others are still repaired. The plans and memory
/// it brought along stay, as they only add what the profile lacked. A
/// transcript it shares with a session repaired before it has moved already
/// and is left where it is now.
///
/// `home`'s desktop app must not run while its config dir is written, so one
/// that started again since the check is looked for right before the first
/// write, and the repair refused if it runs, and again before each session,
/// which is skipped if it runs by then. The processes are listed for that at
/// most every [`LISTING_FRESH_FOR`]. A terminal or another desktop app may
/// have opened a session since, so that is looked for right before each
/// session moves: a process that has a session open registers it, so only a
/// session something registered has the running processes listed afresh.
pub fn apply(prepared: PreparedRepair, at: DateTime<Utc>) -> AppResult<RepairReport> {
    let mut left = Vec::new();
    let mut report = apply_with(
        prepared,
        at,
        &mut |from, to| {
            if let Some(from) = move_exclusive(from, to)? {
                left.push(still_also_at(to, &from));
            }
            Ok(())
        },
        &mut process_list,
        LISTING_FRESH_FOR,
    )?;
    report.warnings.extend(left);
    Ok(report)
}

/// What is said of a file moved to `to` whose old name `from` couldn't be
/// unlinked: both names are links to the one file.
pub(super) fn still_also_at(to: &Path, from: &Path) -> String {
    format!("{} is still also at {}", to.display(), from.display())
}

/// The running processes, as `ps -ax -o pid=,command=` lists them, listed
/// with a closure and kept while current.
struct Listing<'a, P: FnMut() -> AppResult<String>> {
    /// Lists them.
    processes: &'a mut P,
    /// How long a listing stays current.
    fresh_for: Duration,
    /// The last listing, with when it was made.
    last: Option<(Instant, String)>,
}

impl<P: FnMut() -> AppResult<String>> Listing<'_, P> {
    /// The last listing while it is current, else a new one.
    fn recent(&mut self) -> AppResult<String> {
        match &self.last {
            Some((made, listed)) if made.elapsed() < self.fresh_for => Ok(listed.clone()),
            _ => self.fresh(),
        }
    }

    /// A new listing.
    fn fresh(&mut self) -> AppResult<String> {
        let listed = (self.processes)()?;
        self.last = Some((Instant::now(), listed.clone()));
        Ok(listed)
    }
}

/// [`apply()`], moving each file or folder with `rename` and listing the
/// running processes, as `ps -ax -o pid=,command=` does, with `processes`, a
/// listing staying current for `fresh_for`.
pub(super) fn apply_with(
    prepared: PreparedRepair,
    at: DateTime<Utc>,
    rename: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
    processes: &mut impl FnMut() -> AppResult<String>,
    fresh_for: Duration,
) -> AppResult<RepairReport> {
    let PreparedRepair {
        home,
        homes,
        sessions,
        mut skipped,
    } = prepared;
    if !sessions.is_empty() {
        refuse_running(&home)?;
    }
    let mut listing = Listing {
        processes,
        fresh_for,
        last: None,
    };
    let mut repaired: u32 = 0;
    let mut memory_conflicts: Vec<String> = Vec::new();
    for session in sessions {
        let outcome = still_free(&session, &home, &homes, &mut listing).and_then(|()| {
            let backup = replaced_dir(&home.config_dir, &session.session_id, at)
                .map_err(|error| error.message())?;
            repair_session(&home.config_dir, &session, &backup, at, rename)
                .map_err(|error| failure(&error, &backup))
        });
        match outcome {
            Ok(conflicts) => {
                repaired += 1;
                for conflict in conflicts {
                    if !memory_conflicts.contains(&conflict) {
                        memory_conflicts.push(conflict);
                    }
                }
            }
            Err(reason) => skipped.push(SkippedSession {
                id: session.session_id,
                reason,
            }),
        }
    }
    Ok(RepairReport {
        repaired,
        skipped,
        memory_conflicts,
        warnings: Vec::new(),
    })
}

/// Whether `session` of `home`, one of `homes`, is still free to move: `Err`
/// with the reason when `home`'s desktop app runs by the `listing`'s recent
/// processes, or when [`in_use`]. The processes are listed afresh for that
/// only when a process registered one of its transcripts in any home.
/// Processes that can't be listed keep the session where it is.
fn still_free<P: FnMut() -> AppResult<String>>(
    session: &SessionRepair,
    home: &Home,
    homes: &[Home],
    listing: &mut Listing<P>,
) -> Result<(), String> {
    let recent = listing.recent().map_err(|error| error.message())?;
    if desktop_pid(home, &recent).is_some() {
        return Err(running_again(home).message());
    }
    let registered = homes.iter().any(|each| {
        registrations(&each.config_dir)
            .iter()
            .any(|registration| session.transcript_ids.contains(&registration.session_id))
    });
    if !registered {
        return Ok(());
    }
    let ps_output = listing.fresh().map_err(|error| error.message())?;
    match in_use(
        &session.transcript_ids,
        home,
        &live_by_home(homes, &ps_output),
    ) {
        Some(reason) => Err(reason),
        None => Ok(()),
    }
}

/// A step of a session's repair, to take back should a later one fail.
#[derive(Debug)]
enum Step {
    /// These folders were made in the config dir to put items in, the
    /// outermost first.
    MadeFolders {
        /// The folders.
        folders: Vec<PathBuf>,
    },
    /// These items were moved here from the config dir `from`.
    Moved {
        /// The config dir they came from.
        from: PathBuf,
        /// The items, relative to either config dir.
        items: Vec<PathBuf>,
    },
    /// This item was copied here; its original is still where it was.
    Copied {
        /// The item, relative to the config dir.
        item: PathBuf,
    },
    /// Transcript `session_id` was archived in the config dir `from`, once
    /// copied here.
    Archived {
        /// The config dir it was archived in.
        from: PathBuf,
        /// The transcript's id.
        session_id: String,
    },
}

/// Repair `session` into `config_dir`, backing up what memory merging
/// rewrites into `backup`, moving each file or folder with `rename`, or, across
/// volumes, archiving at `at` what was copied. A transcript that fails to
/// move has the ones before it put back; what was copied of them is set
/// aside into `backup`'s undone folder. Returns the memory conflicts.
fn repair_session(
    config_dir: &Path,
    session: &SessionRepair,
    backup: &Path,
    at: DateTime<Utc>,
    rename: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> AppResult<Vec<String>> {
    for plan in &session.plans {
        if !occupied(&config_dir.join(&plan.relative)) {
            place(&plan.from, config_dir, &plan.relative, backup)?;
        }
    }
    let mut conflicts = Vec::new();
    for merge in &session.memory {
        conflicts.extend(merge_memory(
            &merge.from,
            &config_dir.join(&merge.relative),
            &backup.join(&merge.relative),
            // What merging adds stays, so it isn't logged to take back.
            &mut Vec::new(),
        )?);
    }
    let mut steps = Vec::new();
    for transcript in &session.transcripts {
        if let Err(error) = relocate(transcript, config_dir, backup, at, rename, &mut steps) {
            let unwound = unwind(steps, config_dir, backup, rename);
            if unwound.is_empty() {
                return Err(error);
            }
            return Err(AppError::Validation(format!(
                "{}. Putting the session back failed too: {}",
                error.message(),
                unwound.join("; ")
            )));
        }
    }
    Ok(conflicts)
}

/// Move `transcript`'s items into `config_dir` with `rename`, logging each
/// step taken in `steps`. Items already there the same are left, as is one
/// gone from where it was that is there now, which a session sharing the
/// transcript moved. Across volumes, where a rename can't go, the items are
/// copied there instead, then archived where they were at `at`, so they are
/// out of the way and can be restored. What is replaced on the way, which is
/// only what showed up since the check, is backed up into `backup`.
fn relocate(
    transcript: &TranscriptMove,
    config_dir: &Path,
    backup: &Path,
    at: DateTime<Utc>,
    rename: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
    steps: &mut Vec<Step>,
) -> AppResult<()> {
    let from = &transcript.config_dir;
    let mut pending = Vec::new();
    for item in &transcript.items {
        let (source, target) = (from.join(item), config_dir.join(item));
        match (occupied(&source), occupied(&target)) {
            (true, false) => pending.push(item.clone()),
            (false, true) => {}
            (true, true) if compare(&source, &target)? == ItemAction::Same => {}
            (true, true) => {
                return Err(AppError::Validation(format!(
                    "{} already exists",
                    target.display()
                )))
            }
            (false, false) => {
                return Err(AppError::Validation(format!(
                    "{} is gone",
                    source.display()
                )))
            }
        }
    }
    if pending.is_empty() {
        return Ok(());
    }
    let folders = missing_folders(&pending, config_dir);
    if !folders.is_empty() {
        steps.push(Step::MadeFolders { folders });
    }
    let failed = match move_all(&pending, from, config_dir, rename) {
        Ok(()) => {
            steps.push(Step::Moved {
                from: from.clone(),
                items: pending,
            });
            return Ok(());
        }
        Err(failed) => failed,
    };
    if !failed.stranded.is_empty() {
        return Err(failed.stranded_error(&format!("so they stay in {}", config_dir.display())));
    }
    if !matches!(&failed.error, AppError::Io(error) if error.kind() == io::ErrorKind::CrossesDevices)
    {
        return Err(failed.error);
    }
    for item in &pending {
        place(&from.join(item), config_dir, item, backup)?;
        steps.push(Step::Copied { item: item.clone() });
    }
    let paths: Vec<PathBuf> = pending.iter().map(|item| from.join(item)).collect();
    archive_bundle(from, &transcript.described, &paths, at)?;
    steps.push(Step::Archived {
        from: from.clone(),
        session_id: transcript.described.session_id.clone(),
    });
    Ok(())
}

/// The folders under `config_dir` that putting `items` there makes, as they
/// aren't there yet, the outermost first.
fn missing_folders(items: &[PathBuf], config_dir: &Path) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = Vec::new();
    for item in items {
        let mut ancestors: Vec<PathBuf> = item
            .ancestors()
            .skip(1)
            .filter(|ancestor| !ancestor.as_os_str().is_empty())
            .map(|ancestor| config_dir.join(ancestor))
            .take_while(|folder| !occupied(folder))
            .collect();
        ancestors.reverse();
        for folder in ancestors {
            if !folders.contains(&folder) {
                folders.push(folder);
            }
        }
    }
    folders.sort_by_key(|folder| folder.components().count());
    folders
}

/// Take back `steps`, the last first: move what was moved into `config_dir`
/// back with `rename`, restore what was archived, set what was copied aside
/// into `backup`'s undone folder, where it never meets what `backup` backs
/// up, and remove the folders made for them once empty. Returns
/// what couldn't be taken back, naming what stays where.
fn unwind(
    steps: Vec<Step>,
    config_dir: &Path,
    backup: &Path,
    rename: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> Vec<String> {
    let mut failures = Vec::new();
    for step in steps.into_iter().rev() {
        match step {
            Step::MadeFolders { folders } => {
                // Only an empty folder is removed: one still holding what
                // couldn't be put back stays, and that is named already.
                for folder in folders.iter().rev() {
                    let _ = fs::remove_dir(folder);
                }
            }
            Step::Moved { from, items } => {
                for item in items.iter().rev() {
                    if let Err(failed) =
                        move_all(std::slice::from_ref(item), config_dir, &from, rename)
                    {
                        failures.push(stays(item, &failed.error, config_dir));
                    }
                }
            }
            Step::Copied { item } => {
                let set_aside = move_all(
                    std::slice::from_ref(&item),
                    config_dir,
                    &backup.join(UNDONE_DIR),
                    &mut |from, to| fs::rename(from, to),
                );
                if let Err(failed) = set_aside {
                    failures.push(stays(&item, &failed.error, config_dir));
                }
            }
            Step::Archived { from, session_id } => {
                if let Err(error) = restore_bundle(&from, &session_id) {
                    failures.push(format!(
                        "{} stays archived in {} ({})",
                        session_id,
                        from.display(),
                        error.message()
                    ));
                }
            }
        }
    }
    failures
}

/// What is said of `item` that `error` kept from being taken back out of
/// `config_dir`.
fn stays(item: &Path, error: &AppError, config_dir: &Path) -> String {
    format!(
        "{} couldn't be put back ({}), so it stays in {}",
        item.display(),
        error.message(),
        config_dir.display()
    )
}

/// Move `from` to `to`, which must be free: a file with [`move_new`], which
/// never replaces one that showed up since, anything else with a rename. Both
/// fail across volumes. Returns where a copy of a file was left too, if its
/// old place couldn't be unlinked.
pub(super) fn move_exclusive(from: &Path, to: &Path) -> io::Result<Option<PathBuf>> {
    if fs::symlink_metadata(from)?.is_file() {
        return move_new(from, to);
    }
    fs::rename(from, to).map(|()| None)
}

/// Why a session's repair failed with `error`, saying where what it set
/// aside is, when it set anything aside into `backup`.
fn failure(error: &AppError, backup: &Path) -> String {
    if !backup.exists() {
        return error.message();
    }
    format!(
        "{}. What the repair set aside is kept in {}",
        error.message(),
        backup.display()
    )
}
