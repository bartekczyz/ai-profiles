//! Repairing desktop sessions started before profiles had their own folder.
//!
//! Before profiles pointed their desktop app at their own config dir, a
//! profile's desktop app wrote the transcripts of its Code tab sessions into
//! the stock `~/.claude`. Its records of them stay in the profile's data dir,
//! and the app now looks for their transcripts in the profile's config dir,
//! where they aren't, so it opens them empty. [`check`] finds those sessions
//! and [`apply`] moves every transcript each one claims (see
//! [`super::ownership`]) into the profile's config dir, at the same path
//! relative to it, bringing the project's memory and the plans the
//! transcripts wrote along. The desktop records are left as they are.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::archive_store::{
    archive_bundle, is_session_dir_name, move_all, occupied, replaced_dir, restore_bundle,
    ArchivedBundle,
};
use super::copy::{compare, move_new, place, ItemAction};
use super::live::{live_sessions, registrations, LiveHolder};
use super::memory::merge_memory;
use super::ownership::{
    claimed_copy, claimed_ids, copies, owned_by, HeldTranscript, HomeScan, Owned,
};
use super::transcript::bundle_paths;
use super::transfer::{memory_merges, refuse_running, relative_to, MemoryMerge, PLANS_DIR};
use crate::error::{AppError, AppResult};
use crate::launch::process_list;
use crate::sessions::actions::{ActionCheck, AppToQuit, Checked};
use crate::sessions::instance::{desktop_label, desktop_pid};
use crate::sessions::list::{home_scans, transcript_title, OPEN_IN_TERMINAL};
use crate::sessions::Home;

/// A session a repair left as it was, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedSession {
    /// The session's id.
    pub id: String,
    /// Why it was left as it was.
    pub reason: String,
}

/// What a repair did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairReport {
    /// How many sessions were repaired.
    pub repaired: u32,
    /// The sessions that needed repair but were left as they were.
    pub skipped: Vec<SkippedSession>,
    /// The memory files both folders have, differently; the profile's own
    /// were kept.
    pub memory_conflicts: Vec<String>,
}

/// One transcript to move into the repaired home's config dir.
#[derive(Debug, Clone)]
struct TranscriptMove {
    /// The config dir holding it now.
    config_dir: PathBuf,
    /// Its files and folders, relative to either config dir, the transcript
    /// last.
    items: Vec<PathBuf>,
    /// What archiving it where it is says of it, should it have to be copied
    /// rather than moved.
    described: ArchivedBundle,
}

/// A plan file to copy into the repaired home's config dir.
#[derive(Debug, Clone)]
struct PlanCopy {
    /// Where it is now.
    from: PathBuf,
    /// Where it goes, relative to the repaired home's config dir.
    relative: PathBuf,
}

/// What repairing one session does.
#[derive(Debug, Clone)]
struct SessionRepair {
    /// The session's id.
    session_id: String,
    /// The ids of all its transcripts, wherever they are: what a terminal
    /// having it open would register.
    transcript_ids: Vec<String>,
    /// The plan files its transcripts wrote that the home lacks.
    plans: Vec<PlanCopy>,
    /// The memory folders of the projects its transcripts are in.
    memory: Vec<MemoryMerge>,
    /// Its transcripts in other homes' config dirs, the one shown last.
    transcripts: Vec<TranscriptMove>,
}

/// A checked repair, with what [`apply`] needs to carry it out.
#[derive(Debug)]
pub struct PreparedRepair {
    /// The home whose sessions are repaired.
    home: Home,
    /// Every home of the app.
    homes: Vec<Home>,
    /// The sessions to repair.
    sessions: Vec<SessionRepair>,
    /// The sessions that need repair but can't have it now.
    skipped: Vec<SkippedSession>,
}

/// What repairing the sessions of `home`, one of `homes`, would do, given the
/// output of `ps -ax -o pid=,command=`.
///
/// An active session of `home` needs repair when a transcript it claims is in
/// another home's config dir; archived ones are left as they are. One is
/// skipped, and said why:
/// - while a terminal has one of its transcripts open, or another home's
///   desktop app does, as only `home`'s is quit;
/// - while another home's active desktop record claims the same copy of one
///   of them, as moving it would take it from there; a home keeping its own
///   copy, as one a session was moved to does, claims that one instead, and
///   does so archived too, as Restore there brings it back;
/// - when one of them has an id that isn't a single plain folder name, as its
///   files are found by it;
/// - when `home` has different files where one of them goes: a bulk repair
///   never replaces what a profile has, which a Move of that one session can,
///   with the user's say-so.
///
/// Files `home` has the same of are left in both places. Only `home`'s own
/// desktop app has to quit, as only its config dir is written, and only when
/// there is something to repair.
pub fn check(home: &Home, homes: &[Home], ps_output: &str) -> AppResult<Checked<PreparedRepair>> {
    let scans = home_scans(homes);
    let live = live_by_home(homes, ps_output);
    let listers = listers(&scans);
    let context = Context {
        home,
        homes,
        live: &live,
        listers: &listers,
    };
    let mut sessions = Vec::new();
    let mut skipped = Vec::new();
    for owned in owned_by(&home.id, &scans) {
        let archived = owned.record.as_ref().is_some_and(|record| record.archived);
        let orphaned = owned
            .claimed_transcripts
            .iter()
            .any(|held| held.home_id != home.id);
        if archived || !orphaned {
            continue;
        }
        match session_repair(&context, &owned) {
            Ok(repair) => sessions.push(repair),
            Err(reason) => skipped.push(SkippedSession {
                id: owned.session_id,
                reason,
            }),
        }
    }
    let app_to_quit = (!sessions.is_empty() && desktop_pid(home, ps_output).is_some())
        .then(|| AppToQuit::of(home));
    Ok(Checked {
        check: ActionCheck {
            blocker: None,
            app_to_quit,
        },
        target: PreparedRepair {
            home: home.clone(),
            homes: homes.to_vec(),
            sessions,
            skipped,
        },
    })
}

/// Carry out `prepared`, a repair [`check`] found, at `at`: for each session,
/// copy the plans its transcripts wrote, merge its projects' memory, then move
/// its transcripts, the one shown last. Between homes on one volume a move is
/// a rename; across volumes each transcript is copied, then archived where it
/// was (see [`super::archive_store`]).
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
/// write, and the repair refused if it runs. A terminal or another desktop
/// app may have opened a session since, so that is looked for right before
/// each session moves: a process that has a session open registers it, so
/// only a session something registered has the running processes listed.
pub fn apply(prepared: PreparedRepair, at: DateTime<Utc>) -> AppResult<RepairReport> {
    apply_with(prepared, at, &mut move_exclusive, &mut process_list)
}

/// [`apply`], moving each file or folder with `rename` and listing the
/// running processes, as `ps -ax -o pid=,command=` does, with `processes`.
fn apply_with(
    prepared: PreparedRepair,
    at: DateTime<Utc>,
    rename: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
    processes: &mut impl FnMut() -> AppResult<String>,
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
    let mut repaired: u32 = 0;
    let mut memory_conflicts: Vec<String> = Vec::new();
    for session in sessions {
        let outcome = still_free(&session, &home, &homes, processes).and_then(|()| {
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
    })
}

/// What [`check`] looks up each session in.
struct Context<'a> {
    /// The home whose sessions are repaired.
    home: &'a Home,
    /// Every home of the app.
    homes: &'a [Home],
    /// The sessions open in each home's config dir, by transcript id.
    live: &'a [(&'a Home, HashMap<String, LiveHolder>)],
    /// The homes whose active desktop records claim each copy of a
    /// transcript, by its id and the home holding the copy.
    listers: &'a HashMap<(String, String), Vec<String>>,
}

/// The ids of the homes whose desktop records claim each copy of a
/// transcript, by the transcript's id and the id of the home holding the
/// copy. A record claims its own home's copy when there is one (see
/// [`claimed_copy`]), so a home that keeps its own copy of a transcript
/// doesn't list the one a repair takes. An archived record counts only for
/// its own home's copy: that is its session's, to restore, while one in
/// another home is left to be repaired.
fn listers(scans: &[HomeScan]) -> HashMap<(String, String), Vec<String>> {
    let copies = copies(scans);
    let mut listers: HashMap<(String, String), Vec<String>> = HashMap::new();
    for scan in scans {
        for (record, id) in scan
            .records
            .iter()
            .flat_map(|record| claimed_ids(record).map(move |id| (record, id)))
        {
            let Some(copy) = claimed_copy(&scan.home_id, id, &copies) else {
                continue;
            };
            if record.archived && copy != scan.home_id {
                continue;
            }
            let homes = listers
                .entry((id.to_string(), copy.to_string()))
                .or_default();
            if !homes.contains(&scan.home_id) {
                homes.push(scan.home_id.clone());
            }
        }
    }
    listers
}

/// The sessions open in each of `homes`' config dirs, by transcript id,
/// given the output of `ps -ax -o pid=,command=`. A process registers the
/// session it has open in the config dir its transcript is in.
fn live_by_home<'a>(
    homes: &'a [Home],
    ps_output: &str,
) -> Vec<(&'a Home, HashMap<String, LiveHolder>)> {
    homes
        .iter()
        .map(|each| (each, live_sessions(&each.config_dir, ps_output)))
        .collect()
}

/// Why a session of `home` whose transcripts are `ids` can't be moved while
/// the sessions `live` in each home are open, if it can't: a terminal has one
/// open, or another home's desktop app does. `home`'s own desktop app quits
/// before anything moves.
fn in_use(
    ids: &[String],
    home: &Home,
    live: &[(&Home, HashMap<String, LiveHolder>)],
) -> Option<String> {
    let mut reason = None;
    for (each, open) in live {
        for id in ids {
            match open.get(id) {
                Some(LiveHolder::Terminal) => return Some(OPEN_IN_TERMINAL.to_string()),
                Some(LiveHolder::Desktop) if each.id != home.id && reason.is_none() => {
                    reason = Some(format!("{} has it open", desktop_label(each)));
                }
                _ => {}
            }
        }
    }
    reason
}

/// Whether `session` of `home`, one of `homes`, is still free to move: `Err`
/// with the reason when [`in_use`]. The running processes are listed with
/// `processes` only when a process registered one of its transcripts in any
/// home; one that can't be listed then keeps the session where it is.
fn still_free(
    session: &SessionRepair,
    home: &Home,
    homes: &[Home],
    processes: &mut impl FnMut() -> AppResult<String>,
) -> Result<(), String> {
    let registered = homes.iter().any(|each| {
        registrations(&each.config_dir)
            .iter()
            .any(|registration| session.transcript_ids.contains(&registration.session_id))
    });
    if !registered {
        return Ok(());
    }
    let ps_output = processes().map_err(|error| error.message())?;
    match in_use(
        &session.transcript_ids,
        home,
        &live_by_home(homes, &ps_output),
    ) {
        Some(reason) => Err(reason),
        None => Ok(()),
    }
}

/// What repairing `owned`, a session of the context's home, does, or why it
/// can't be repaired now.
fn session_repair(context: &Context, owned: &Owned) -> Result<SessionRepair, String> {
    let home = context.home;
    let displayed = owned
        .transcript
        .as_ref()
        .map(|held| held.summary.session_id.as_str());
    let mut orphans: Vec<&HeldTranscript> = owned
        .claimed_transcripts
        .iter()
        .filter(|held| held.home_id != home.id)
        .collect();
    // The transcript shown goes last, so a session cut short is put back
    // from the end it got to.
    orphans.sort_by_key(|held| Some(held.summary.session_id.as_str()) == displayed);
    let mut transcript_ids = vec![owned.session_id.clone()];
    for held in &owned.claimed_transcripts {
        if !transcript_ids.contains(&held.summary.session_id) {
            transcript_ids.push(held.summary.session_id.clone());
        }
    }
    if let Some(reason) = in_use(&transcript_ids, home, context.live) {
        return Err(reason);
    }
    for held in &orphans {
        let other = context
            .listers
            .get(&(held.summary.session_id.clone(), held.home_id.clone()))
            .into_iter()
            .flatten()
            .find(|lister| **lister != home.id);
        if let Some(other) = other {
            let label = context
                .homes
                .iter()
                .find(|each| each.id == *other)
                .map_or_else(|| other.clone(), desktop_label);
            return Err(format!("{label} lists it too"));
        }
    }
    let title = owned
        .record
        .as_ref()
        .and_then(|record| record.title.clone());
    let mut transcripts = Vec::new();
    let mut plans: Vec<PlanCopy> = Vec::new();
    for held in &orphans {
        let holder = context
            .homes
            .iter()
            .find(|each| each.id == held.home_id)
            .ok_or_else(|| {
                format!(
                    "Its transcript {:?} is in a profile that's gone",
                    held.summary.session_id
                )
            })?;
        let shown = Some(held.summary.session_id.as_str()) == displayed;
        transcripts.push(transcript_move(
            home,
            holder,
            held,
            title.as_deref(),
            shown,
        )?);
        for slug in &held.summary.plan_slugs {
            let relative = Path::new(PLANS_DIR).join(format!("{slug}.md"));
            let from = holder.config_dir.join(&relative);
            let listed = plans.iter().any(|plan| plan.relative == relative);
            if from.is_file() && !listed && !occupied(&home.config_dir.join(&relative)) {
                plans.push(PlanCopy { from, relative });
            }
        }
    }
    Ok(SessionRepair {
        session_id: owned.session_id.clone(),
        transcript_ids,
        plans,
        memory: memory_merges(orphans, context.homes),
        transcripts,
    })
}

/// What moving `held`, a transcript in `holder`'s config dir, into `home`'s
/// does: the files and folders of its bundle `home` lacks, the transcript
/// last. Should the transcript be copied, it is archived under the title of
/// its session, `title`, else its own; one the session isn't `shown` by is
/// marked the session's earlier part, so the two tell apart in the Archived
/// list. Refused, with the reason, for a transcript whose id isn't a single
/// plain folder name, as its bundle's paths are made of it, and when `home`
/// has different files where one goes.
fn transcript_move(
    home: &Home,
    holder: &Home,
    held: &HeldTranscript,
    title: Option<&str>,
    shown: bool,
) -> Result<TranscriptMove, String> {
    let summary = &held.summary;
    if !is_session_dir_name(&summary.session_id) {
        return Err(format!(
            "Its transcript {:?} has an id that isn't safe to move",
            summary.session_id
        ));
    }
    let mut paths = bundle_paths(&holder.config_dir, summary);
    // The transcript is first of its bundle; it goes last.
    paths.rotate_left(1);
    let mut items = Vec::new();
    for path in paths {
        let relative = relative_to(&path, &holder.config_dir).map_err(|error| error.message())?;
        let action =
            compare(&path, &home.config_dir.join(&relative)).map_err(|error| error.message())?;
        match action {
            ItemAction::Copy => items.push(relative),
            ItemAction::Same => {}
            ItemAction::Replace => {
                return Err(format!("{} already has different files of it", home.label))
            }
        }
    }
    let title = title
        .map(str::to_string)
        .or_else(|| transcript_title(summary))
        .map(|title| {
            if shown {
                title
            } else {
                format!("{title} (earlier part)")
            }
        });
    let described = ArchivedBundle {
        session_id: summary.session_id.clone(),
        title,
        cwd: summary.cwd.clone(),
        last_prompt: summary.last_prompt.clone(),
        last_used_at: summary.last_used_at,
    };
    Ok(TranscriptMove {
        config_dir: holder.config_dir.clone(),
        items,
        described,
    })
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
/// aside into `backup`. Returns the memory conflicts.
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
/// into `backup`, and remove the folders made for them once empty. Returns
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
                    backup,
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
/// fail across volumes.
fn move_exclusive(from: &Path, to: &Path) -> io::Result<()> {
    if fs::symlink_metadata(from)?.is_file() {
        return move_new(from, to);
    }
    fs::rename(from, to)
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs::{self, File};
    use std::time::{Duration, SystemTime};

    use serde_json::{json, Value};
    use tempfile::tempdir;

    use super::*;
    use crate::app_kind::AppKind;
    use crate::error::AppError;
    use crate::sessions::actions::{AppToQuit, SessionAction};
    use crate::sessions::claude::archive_store::archived_bundles;
    use crate::sessions::claude::{archive, transfer};
    use crate::sessions::list::claude_sessions;
    use crate::test_support::fake_wrapper_process;

    const ACCOUNT: &str = "a99c6b36-dd42-44d7-b3ae-9496265549fd";
    const ORG: &str = "527aadd2-01c3-49a6-a770-e65e047242c3";

    /// When the tests' transcripts were last written.
    const WRITTEN: u64 = 1_780_000_000;

    /// When the tests repair.
    const AT: &str = "2026-09-24T08:15:00Z";

    /// A Claude home named `name`, under `root`, whose desktop app has been
    /// opened.
    fn home(root: &Path, name: &str) -> Home {
        let home = Home {
            id: name.to_lowercase(),
            app: AppKind::Claude,
            label: name.to_string(),
            config_dir: root.join(name).join("cli-config"),
            gui_data_dir: root.join(name).join("gui-data"),
            stock: false,
        };
        fs::create_dir_all(&home.gui_data_dir).unwrap();
        home
    }

    /// Write `contents` to `path`, making its folder, dated [`WRITTEN`].
    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(written())
            .unwrap();
    }

    /// [`WRITTEN`], as a time.
    fn written() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(WRITTEN)
    }

    /// Where transcript `session` of the tests is, relative to a config dir.
    fn transcript_path(session: &str) -> String {
        format!("projects/-work-app/{session}.jsonl")
    }

    /// Gives `home` transcript `session`, with a subagent transcript beside it
    /// and a file history.
    fn transcript(home: &Home, session: &str) {
        let line = json!({
            "type": "user",
            "sessionId": session,
            "timestamp": "2026-09-01T10:00:00Z",
            "cwd": "/work/app",
            "message": { "role": "user", "content": "Fix the login bug" },
        });
        let config = &home.config_dir;
        write(&config.join(transcript_path(session)), &format!("{line}\n"));
        write(
            &config.join(format!(
                "projects/-work-app/{session}/subagents/agent-1.jsonl"
            )),
            "{}\n",
        );
        write(&config.join(format!("file-history/{session}/abc@v1")), "v1");
    }

    /// The paths of transcript `session`'s bundle, relative to a config dir.
    fn bundle(session: &str) -> [String; 3] {
        [
            transcript_path(session),
            format!("projects/-work-app/{session}/subagents/agent-1.jsonl"),
            format!("file-history/{session}/abc@v1"),
        ]
    }

    /// Where `home`'s desktop record `local_<uuid>` is.
    fn record_path(home: &Home, uuid: &str) -> PathBuf {
        home.gui_data_dir
            .join("claude-code-sessions")
            .join(ACCOUNT)
            .join(ORG)
            .join(format!("local_{uuid}.json"))
    }

    /// Writes `home`'s desktop record `local_<uuid>` of `fields`.
    fn record(home: &Home, uuid: &str, fields: Value) {
        write(&record_path(home, uuid), &fields.to_string());
    }

    /// Every file under `root`, with its contents.
    fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut files = BTreeMap::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(dir) = pending.pop() {
            for entry in fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if entry.file_type().unwrap().is_dir() {
                    pending.push(path);
                } else {
                    files.insert(path.clone(), fs::read(&path).unwrap());
                }
            }
        }
        files
    }

    /// Checks the repair of `home`'s sessions, then carries it out.
    fn repair(home: &Home, homes: &[Home], ps_output: &str) -> RepairReport {
        let checked = check(home, homes, ps_output).unwrap();
        apply(checked.target, AT.parse().unwrap()).unwrap()
    }

    /// The Default home with two transcripts Personal's desktop app started
    /// there, `now` continuing `before`, and one of its own, `cli`.
    fn orphaned(root: &Path) -> (Home, Home, Vec<Home>) {
        let default = home(root, "Default");
        let personal = home(root, "Personal");
        transcript(&default, "before");
        transcript(&default, "now");
        transcript(&default, "cli");
        record(
            &personal,
            "r1",
            json!({ "cliSessionId": "now", "priorCliSessionIds": ["before"], "cwd": "/work/app" }),
        );
        let homes = vec![default.clone(), personal.clone()];
        (default, personal, homes)
    }

    #[test]
    fn repair_moves_every_transcript_a_session_claims_into_the_profile() {
        let root = tempdir().unwrap();
        let (default, personal, homes) = orphaned(root.path());
        let record_before = fs::read(record_path(&personal, "r1")).unwrap();
        assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 1);

        let report = repair(&personal, &homes, "");

        assert_eq!(
            report,
            RepairReport {
                repaired: 1,
                ..RepairReport::default()
            }
        );
        let listed = claude_sessions(&personal, &homes, "");
        assert_eq!(listed.repair_count, 0);
        assert_eq!(listed.sessions.len(), 1);
        assert_eq!(listed.sessions[0].id, "now");
        for session in ["before", "now"] {
            for path in bundle(session) {
                let moved = personal.config_dir.join(&path);
                assert!(moved.exists(), "{path} isn't in the profile");
                assert_eq!(fs::metadata(&moved).unwrap().modified().unwrap(), written());
            }
        }
        assert_eq!(
            fs::read(record_path(&personal, "r1")).unwrap(),
            record_before
        );
        assert!(default.config_dir.join(transcript_path("cli")).exists());
    }

    #[test]
    fn the_default_home_no_longer_holds_what_was_repaired() {
        let root = tempdir().unwrap();
        let (default, personal, homes) = orphaned(root.path());

        repair(&personal, &homes, "");

        for session in ["before", "now"] {
            for path in bundle(session) {
                assert!(
                    !default.config_dir.join(&path).exists(),
                    "{path} is still in Default"
                );
            }
            assert!(!default
                .config_dir
                .join(format!("projects/-work-app/{session}"))
                .exists());
        }
        assert_eq!(archived_bundles(&default.config_dir), []);
        let listed = claude_sessions(&default, &homes, "");
        let ids: Vec<&str> = listed
            .sessions
            .iter()
            .map(|session| session.id.as_str())
            .collect();
        assert_eq!(ids, ["cli"]);
    }

    #[test]
    fn a_session_open_in_a_terminal_is_skipped_and_the_rest_repaired() {
        let root = tempdir().unwrap();
        let (default, personal, homes) = orphaned(root.path());
        transcript(&default, "other");
        record(&personal, "r2", json!({ "cliSessionId": "other" }));
        write(
            &default.config_dir.join("sessions/4100.json"),
            &json!({ "pid": 4100, "sessionId": "before", "entrypoint": "cli" }).to_string(),
        );

        let report = repair(&personal, &homes, "  4100 claude\n");

        assert_eq!(report.repaired, 1);
        assert_eq!(
            report.skipped,
            [SkippedSession {
                id: "now".to_string(),
                reason: "Close it in the terminal first".to_string(),
            }]
        );
        assert!(personal.config_dir.join(transcript_path("other")).exists());
        assert!(default.config_dir.join(transcript_path("now")).exists());
        assert!(default.config_dir.join(transcript_path("before")).exists());
        assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 1);
    }

    #[test]
    fn a_second_repair_changes_nothing() {
        let root = tempdir().unwrap();
        let (_, personal, homes) = orphaned(root.path());
        repair(&personal, &homes, "");
        let before = tree(root.path());
        let running = format!(
            "  901 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={}\n",
            personal.gui_data_dir.display()
        );

        let checked = check(&personal, &homes, &running).unwrap();
        let report = apply(checked.target, AT.parse().unwrap()).unwrap();

        assert_eq!(checked.check, ActionCheck::default());
        assert_eq!(report, RepairReport::default());
        assert_eq!(tree(root.path()), before);
        assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 0);
    }

    #[test]
    fn across_volumes_the_transcripts_are_copied_and_archived_where_they_were() {
        let root = tempdir().unwrap();
        let (default, personal, homes) = orphaned(root.path());
        let checked = check(&personal, &homes, "").unwrap();

        let report = apply_with(
            checked.target,
            AT.parse().unwrap(),
            &mut |_, _| Err(io::ErrorKind::CrossesDevices.into()),
            &mut || Ok(String::new()),
        )
        .unwrap();

        assert_eq!(report.repaired, 1);
        for session in ["before", "now"] {
            for path in bundle(session) {
                let copied = personal.config_dir.join(&path);
                assert_eq!(
                    fs::metadata(&copied).unwrap().modified().unwrap(),
                    written()
                );
                assert!(!default.config_dir.join(&path).exists());
            }
        }
        let mut archived: Vec<(String, Option<String>)> = archived_bundles(&default.config_dir)
            .into_iter()
            .map(|bundle| (bundle.session_id, bundle.title))
            .collect();
        archived.sort();
        assert_eq!(
            archived,
            [
                (
                    "before".to_string(),
                    Some("Fix the login bug (earlier part)".to_string())
                ),
                ("now".to_string(), Some("Fix the login bug".to_string())),
            ]
        );
        assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 0);
    }

    #[test]
    fn a_session_that_cant_all_be_put_back_says_what_stays_in_the_profile() {
        let root = tempdir().unwrap();
        let (default, personal, homes) = orphaned(root.path());
        let checked = check(&personal, &homes, "").unwrap();
        let into_profile = personal.config_dir.clone();

        // The shown transcript won't move, and then the earlier one won't go
        // back.
        let report = apply_with(
            checked.target,
            AT.parse().unwrap(),
            &mut |from, to| {
                let back = from.starts_with(&into_profile) && from.ends_with("before.jsonl");
                if from.ends_with("now.jsonl") || back {
                    return Err(io::ErrorKind::PermissionDenied.into());
                }
                move_exclusive(from, to)
            },
            &mut || Ok(String::new()),
        )
        .unwrap();

        assert_eq!(report.repaired, 0);
        let reason = &report.skipped[0].reason;
        assert!(
            reason.contains("projects/-work-app/before.jsonl"),
            "{reason}"
        );
        assert!(
            reason.contains(&personal.config_dir.display().to_string()),
            "{reason}"
        );
        assert!(personal.config_dir.join(transcript_path("before")).exists());
        assert!(default
            .config_dir
            .join("file-history/before/abc@v1")
            .exists());
        assert!(default.config_dir.join(transcript_path("now")).exists());
    }

    #[test]
    fn processes_are_only_listed_for_a_session_a_process_registered() {
        let root = tempdir().unwrap();
        let (default, personal, homes) = orphaned(root.path());
        transcript(&default, "other");
        record(&personal, "r2", json!({ "cliSessionId": "other" }));
        let checked = check(&personal, &homes, "").unwrap();
        write(
            &default.config_dir.join("sessions/4100.json"),
            &json!({ "pid": 4100, "sessionId": "other", "entrypoint": "cli" }).to_string(),
        );
        let mut listed = 0;

        let report = apply_with(
            checked.target,
            AT.parse().unwrap(),
            &mut move_exclusive,
            &mut || {
                listed += 1;
                Ok(String::new())
            },
        )
        .unwrap();

        assert_eq!(report.repaired, 2);
        assert_eq!(listed, 1);
    }

    #[test]
    fn a_session_another_profiles_desktop_app_has_open_is_skipped() {
        let root = tempdir().unwrap();
        let (default, personal, homes) = orphaned(root.path());
        write(
            &default.config_dir.join("sessions/4200.json"),
            &json!({ "pid": 4200, "sessionId": "now", "entrypoint": "claude-desktop" }).to_string(),
        );
        let running =
            "  4200 /Users/me/Library/Application Support/Claude/claude-code/2.1.9/claude\n";
        let before = tree(root.path());

        let report = repair(&personal, &homes, running);

        assert_eq!(report.repaired, 0);
        assert_eq!(
            report.skipped,
            [SkippedSession {
                id: "now".to_string(),
                reason: "Claude (Default) has it open".to_string(),
            }]
        );
        assert_eq!(tree(root.path()), before);
    }

    #[test]
    fn a_session_the_profile_has_different_files_of_is_skipped_untouched() {
        let root = tempdir().unwrap();
        let (default, personal, homes) = orphaned(root.path());
        write(
            &personal.config_dir.join("file-history/before/abc@v1"),
            "theirs",
        );
        let before = tree(root.path());

        let report = repair(&personal, &homes, "");

        assert_eq!(report.repaired, 0);
        assert_eq!(
            report.skipped,
            [SkippedSession {
                id: "now".to_string(),
                reason: "Personal already has different files of it".to_string(),
            }]
        );
        assert_eq!(tree(root.path()), before);
        assert!(default.config_dir.join(transcript_path("now")).exists());
    }

    #[test]
    fn a_transcript_another_profiles_desktop_app_lists_too_stays_where_it_is() {
        let root = tempdir().unwrap();
        let (default, personal, mut homes) = orphaned(root.path());
        let work = home(root.path(), "Work");
        record(&work, "w1", json!({ "cliSessionId": "now" }));
        homes.push(work);

        let report = repair(&personal, &homes, "");

        assert_eq!(report.repaired, 0);
        assert_eq!(
            report.skipped,
            [SkippedSession {
                id: "now".to_string(),
                reason: "Claude (Work) lists it too".to_string(),
            }]
        );
        assert!(default.config_dir.join(transcript_path("now")).exists());
    }

    /// Signs `home`'s desktop app in to the tests' account.
    fn sign_in(home: &Home) {
        write(
            &home.gui_data_dir.join("config.json"),
            &json!({ "lastKnownAccountUuid": ACCOUNT }).to_string(),
        );
        write(
            &home.config_dir.join(".claude.json"),
            &json!({ "oauthAccount": { "accountUuid": ACCOUNT, "organizationUuid": ORG } })
                .to_string(),
        );
    }

    #[test]
    fn a_session_restored_after_moving_it_away_is_repaired_and_the_banner_clears() {
        let root = tempdir().unwrap();
        let (default, personal, mut homes) = orphaned(root.path());
        let work = home(root.path(), "Work");
        sign_in(&work);
        homes.push(work.clone());
        let moving = transfer::plan(&personal, &work, &homes, "now", "").unwrap();
        transfer::execute(moving, AT.parse().unwrap()).unwrap();
        let restoring =
            archive::check(&personal, &homes, "now", SessionAction::Restore, "").unwrap();
        archive::apply(&personal, restoring.target, SessionAction::Restore).unwrap();
        assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 1);
        let at_work = tree(&work.config_dir);

        let report = repair(&personal, &homes, "");

        assert_eq!(
            report,
            RepairReport {
                repaired: 1,
                ..RepairReport::default()
            }
        );
        assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 0);
        for session in ["before", "now"] {
            assert!(personal.config_dir.join(transcript_path(session)).exists());
            assert!(!default.config_dir.join(transcript_path(session)).exists());
        }
        assert_eq!(tree(&work.config_dir), at_work);
        let listed = claude_sessions(&work, &homes, "");
        assert_eq!(listed.repair_count, 0);
        assert!(listed.sessions.iter().any(|session| session.id == "now"));
    }

    #[test]
    fn an_archived_record_in_another_profile_doesnt_keep_a_session_from_repair() {
        let root = tempdir().unwrap();
        let (default, personal, mut homes) = orphaned(root.path());
        let work = home(root.path(), "Work");
        record(
            &work,
            "w1",
            json!({ "cliSessionId": "now", "isArchived": true }),
        );
        homes.push(work);

        let report = repair(&personal, &homes, "");

        assert_eq!(report.repaired, 1);
        assert!(!default.config_dir.join(transcript_path("now")).exists());
    }

    #[test]
    fn a_copy_an_archived_record_claims_as_its_own_stays_in_its_profile() {
        let root = tempdir().unwrap();
        let personal = home(root.path(), "Personal");
        let work = home(root.path(), "Work");
        transcript(&personal, "s");
        record(
            &personal,
            "p1",
            json!({ "cliSessionId": "s", "isArchived": true }),
        );
        record(&work, "w1", json!({ "cliSessionId": "s" }));
        let homes = vec![personal.clone(), work.clone()];
        let at_personal = tree(&personal.config_dir);

        let report = repair(&work, &homes, "");

        assert_eq!(report.repaired, 0);
        assert_eq!(
            report.skipped,
            [SkippedSession {
                id: "s".to_string(),
                reason: "Claude (Personal) lists it too".to_string(),
            }]
        );
        assert_eq!(tree(&personal.config_dir), at_personal);
        assert!(!work.config_dir.join(transcript_path("s")).exists());
    }

    #[test]
    fn project_memory_and_plans_come_along_and_memory_conflicts_are_reported() {
        let root = tempdir().unwrap();
        let (default, personal, homes) = orphaned(root.path());
        let memory = "projects/-work-app/memory";
        write(&default.config_dir.join(memory).join("style.md"), "Tabs");
        write(&default.config_dir.join(memory).join("deploy.md"), "Vercel");
        write(
            &personal.config_dir.join(memory).join("deploy.md"),
            "Netlify",
        );
        let now = default.config_dir.join(transcript_path("now"));
        let lines = format!(
            "{}{}\n",
            fs::read_to_string(&now).unwrap(),
            json!({ "type": "assistant", "timestamp": "2026-09-01T10:01:00Z", "slug": "bold-plan" })
        );
        write(&now, &lines);
        write(&default.config_dir.join("plans/bold-plan.md"), "# Plan");

        let report = repair(&personal, &homes, "");

        assert_eq!(report.repaired, 1);
        assert_eq!(report.memory_conflicts, ["deploy.md"]);
        assert_eq!(
            fs::read_to_string(personal.config_dir.join(memory).join("style.md")).unwrap(),
            "Tabs"
        );
        assert_eq!(
            fs::read_to_string(personal.config_dir.join(memory).join("deploy.md")).unwrap(),
            "Netlify"
        );
        assert!(default.config_dir.join(memory).join("style.md").exists());
        assert_eq!(
            fs::read_to_string(personal.config_dir.join("plans/bold-plan.md")).unwrap(),
            "# Plan"
        );
    }

    #[test]
    fn only_the_profiles_own_desktop_app_has_to_quit_and_only_when_something_moves() {
        let root = tempdir().unwrap();
        let (default, personal, homes) = orphaned(root.path());
        let running = format!(
            "  900 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={}\n  \
             901 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={}\n",
            default.gui_data_dir.display(),
            personal.gui_data_dir.display()
        );

        let checked = check(&personal, &homes, &running).unwrap();

        assert_eq!(
            checked.check,
            ActionCheck {
                blocker: None,
                app_to_quit: Some(AppToQuit::of(&personal)),
            }
        );
        assert_eq!(
            check(&default, &homes, &running).unwrap().check,
            ActionCheck::default()
        );
    }

    #[test]
    fn nothing_moves_when_the_profiles_desktop_app_started_again_since_the_check() {
        let root = tempdir().unwrap();
        let (default, personal, homes) = orphaned(root.path());
        let checked = check(&personal, &homes, "").unwrap();
        let before = tree(root.path());
        let mut relaunched = fake_wrapper_process(&root.path().join("app"), &personal.gui_data_dir);

        let repaired = apply(checked.target, AT.parse().unwrap());

        relaunched.kill().unwrap();
        relaunched.wait().unwrap();
        assert!(
            matches!(&repaired, Err(AppError::Validation(message)) if message == "Claude (Personal) is running again — quit it and try again"),
            "{repaired:?}"
        );
        let mut after = tree(root.path());
        after.retain(|path, _| !path.starts_with(root.path().join("app")));
        assert_eq!(after, before);
        assert!(default.config_dir.join(transcript_path("now")).exists());
    }

    #[test]
    fn two_sessions_that_continued_the_same_transcript_are_both_repaired() {
        let root = tempdir().unwrap();
        let (default, personal, homes) = orphaned(root.path());
        transcript(&default, "later");
        record(
            &personal,
            "r2",
            json!({ "cliSessionId": "later", "priorCliSessionIds": ["before"], "cwd": "/work/app" }),
        );

        let report = repair(&personal, &homes, "");

        assert_eq!(
            report,
            RepairReport {
                repaired: 2,
                ..RepairReport::default()
            }
        );
        for session in ["before", "now", "later"] {
            for path in bundle(session) {
                assert!(
                    personal.config_dir.join(&path).exists(),
                    "{path} isn't in the profile"
                );
                assert!(
                    !default.config_dir.join(&path).exists(),
                    "{path} is still in Default"
                );
            }
        }
        assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 0);
    }

    #[test]
    fn an_archived_session_is_neither_counted_nor_repaired() {
        let root = tempdir().unwrap();
        let (default, personal, homes) = orphaned(root.path());
        transcript(&default, "shelved");
        record(
            &personal,
            "r2",
            json!({ "cliSessionId": "shelved", "isArchived": true }),
        );
        assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 1);

        let report = repair(&personal, &homes, "");

        assert_eq!(
            report,
            RepairReport {
                repaired: 1,
                ..RepairReport::default()
            }
        );
        assert!(default.config_dir.join(transcript_path("shelved")).exists());
        assert!(!personal
            .config_dir
            .join(transcript_path("shelved"))
            .exists());
    }

    #[test]
    fn a_session_moves_whole_or_stays_where_it_was() {
        let root = tempdir().unwrap();
        let (default, personal, homes) = orphaned(root.path());
        // The profile has a file history folder already, empty.
        fs::create_dir_all(personal.config_dir.join("file-history")).unwrap();
        let checked = check(&personal, &homes, "").unwrap();
        let before = tree(root.path());

        // The shown transcript moves last, after the earlier one has.
        let report = apply_with(
            checked.target,
            AT.parse().unwrap(),
            &mut |from, to| {
                if from.ends_with("now.jsonl") {
                    return Err(io::ErrorKind::PermissionDenied.into());
                }
                move_exclusive(from, to)
            },
            &mut || Ok(String::new()),
        )
        .unwrap();

        assert_eq!(report.repaired, 0);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].id, "now");
        assert!(
            report.skipped[0].reason.starts_with("permission denied"),
            "{}",
            report.skipped[0].reason
        );
        assert_eq!(tree(root.path()), before);
        assert!(default.config_dir.join(transcript_path("before")).exists());
        assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 1);
        assert!(!personal.config_dir.join("projects").exists());
        assert!(!personal.config_dir.join("file-history/before").exists());
        assert!(personal.config_dir.join("file-history").is_dir());
    }

    #[test]
    fn a_session_claiming_a_transcript_with_an_unsafe_id_is_skipped() {
        let root = tempdir().unwrap();
        let (default, personal, homes) = orphaned(root.path());
        transcript(&default, ".hidden");
        record(
            &personal,
            "r1",
            json!({ "cliSessionId": "now", "priorCliSessionIds": ["before", ".hidden"], "cwd": "/work/app" }),
        );
        let before = tree(root.path());

        let report = repair(&personal, &homes, "");

        assert_eq!(report.repaired, 0);
        assert_eq!(
            report.skipped,
            [SkippedSession {
                id: "now".to_string(),
                reason: "Its transcript \".hidden\" has an id that isn't safe to move".to_string(),
            }]
        );
        assert_eq!(tree(root.path()), before);
    }

    #[test]
    fn a_session_opened_in_a_terminal_since_the_check_is_skipped() {
        let root = tempdir().unwrap();
        let (default, personal, homes) = orphaned(root.path());
        let checked = check(&personal, &homes, "").unwrap();
        write(
            &default.config_dir.join("sessions/4100.json"),
            &json!({ "pid": 4100, "sessionId": "now", "entrypoint": "cli" }).to_string(),
        );

        let report = apply_with(
            checked.target,
            AT.parse().unwrap(),
            &mut move_exclusive,
            &mut || Ok("  4100 claude\n".to_string()),
        )
        .unwrap();

        assert_eq!(report.repaired, 0);
        assert_eq!(
            report.skipped,
            [SkippedSession {
                id: "now".to_string(),
                reason: "Close it in the terminal first".to_string(),
            }]
        );
        assert!(default.config_dir.join(transcript_path("now")).exists());
        assert!(default.config_dir.join(transcript_path("before")).exists());
    }

    #[test]
    fn a_transcript_whose_home_is_unknown_is_a_reason_to_skip() {
        let root = tempdir().unwrap();
        let (_, personal, homes) = orphaned(root.path());
        let scans = home_scans(&homes);
        let owned = owned_by(&personal.id, &scans)
            .into_iter()
            .find(|owned| owned.session_id == "now")
            .unwrap();
        let only_personal = [personal.clone()];
        let context = Context {
            home: &personal,
            homes: &only_personal,
            live: &[],
            listers: &HashMap::new(),
        };

        let repaired = session_repair(&context, &owned);

        assert_eq!(
            repaired.map(|repair| repair.session_id),
            Err("Its transcript \"before\" is in a profile that's gone".to_string())
        );
    }

    #[test]
    fn a_repair_report_crosses_the_bridge_in_camel_case() {
        let report = RepairReport {
            repaired: 2,
            skipped: vec![SkippedSession {
                id: "s".to_string(),
                reason: "Close it in the terminal first".to_string(),
            }],
            memory_conflicts: vec!["deploy.md".to_string()],
        };

        assert_eq!(
            serde_json::to_value(report).unwrap(),
            json!({
                "repaired": 2,
                "skipped": [{ "id": "s", "reason": "Close it in the terminal first" }],
                "memoryConflicts": ["deploy.md"],
            })
        );
    }
}
