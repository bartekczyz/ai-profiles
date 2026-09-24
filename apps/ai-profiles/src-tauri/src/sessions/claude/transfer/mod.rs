//! Moving a Claude session to another profile of the app.
//!
//! [`plan`] says what a move would do without touching anything; [`execute()`]
//! carries out what it planned. A move copies the session's files into the
//! destination's config dir, backing up whatever it replaces, merges the
//! project's memory, copies the transcripts last, adds the session to the
//! destination's desktop app when it is signed in, and finally archives the
//! session at the source, so Restore there undoes it. A move that fails part
//! way takes back what it put at the destination.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::archive::{self, Target};
use super::copy::compare;
use super::desktop::{current_account_dir, read_records, record_account, DesktopRecord};
use super::live::LiveHolder;
use super::memory::merge_copies;
use super::ownership::{owned_by, HeldTranscript, Owned};
use super::transcript::{bundle_paths, TranscriptSummary};
use crate::app_kind::AppKind;
use crate::error::{AppError, AppResult};
use crate::sessions::actions::{AppToQuit, SessionAction};
use crate::sessions::instance::{desktop_pid, running_again, running_desktop_pid};
use crate::sessions::list::{home_scans, live_anywhere, unmovable_reason, SessionState};
use crate::sessions::move_plan::{DesktopAction, ItemAction, MovePlan, MoveReport, PlannedItem};
use crate::sessions::Home;
use chrono::{DateTime, Utc};

mod execute;

pub use execute::execute;

/// A project's memory folder, beside its transcripts.
const MEMORY_DIR: &str = "memory";

/// The folder under a config dir holding the plans sessions wrote.
pub(super) const PLANS_DIR: &str = "plans";

/// The note for a desktop session moving to another account.
const REMOTE_CONTROL_NOTE: &str =
    "On other devices, Remote Control shows only messages sent after the move.";

/// One file or folder to copy.
#[derive(Debug, Clone)]
struct Item {
    /// Where it is now.
    from: PathBuf,
    /// Where it goes, relative to the destination's config dir.
    relative: PathBuf,
    /// What the move does with it.
    action: ItemAction,
    /// For a transcript, copied after everything else: when it was last used.
    used_at: Option<DateTime<Utc>>,
}

/// A project memory folder to merge.
#[derive(Debug, Clone)]
pub(super) struct MemoryMerge {
    /// The source's folder.
    pub(super) from: PathBuf,
    /// The folder, relative to the destination's config dir.
    pub(super) relative: PathBuf,
}

/// The record to write for the destination's desktop app.
#[derive(Debug, Clone)]
struct RecordWrite {
    /// The folder of the account the app is signed in to.
    account_dir: PathBuf,
    /// The source's record of the session, if it has one.
    source: Option<DesktopRecord>,
    /// The transcript the session continues.
    displayed: TranscriptSummary,
    /// The earlier transcripts the move carries.
    priors: Vec<String>,
}

/// A planned move, with what [`execute()`] needs to carry it out.
#[derive(Debug)]
pub struct Prepared {
    /// What the move does, as the user is shown it.
    pub plan: MovePlan,
    /// The session's id.
    session_id: String,
    /// Where the session is.
    source: Home,
    /// Where it goes.
    destination: Home,
    /// What it copies, transcripts last.
    items: Vec<Item>,
    /// The memory folders it merges.
    memory: Vec<MemoryMerge>,
    /// The destination's desktop record, if one is written.
    record: Option<RecordWrite>,
    /// The move writes anything at the destination: files, memory or the
    /// record.
    writes_destination: bool,
    /// What archiving the session at the source is done to.
    archive: Target,
}

/// What moving session `session_id` of `source` to `destination`, both of
/// `homes`, would do, given the output of `ps -ax -o pid=,command=`.
///
/// The move is blocked while it can't be done: the session's transcripts are
/// gone, it works in the desktop app's scratch folder, a terminal has one of
/// its transcripts open, or archiving it at the source is blocked. The source's
/// desktop app has to quit when archiving there writes its record; the
/// destination's when the move writes its record or files it lists. What
/// every home holds is read once, for all of it.
pub fn plan(
    source: &Home,
    destination: &Home,
    homes: &[Home],
    session_id: &str,
    ps_output: &str,
) -> AppResult<Prepared> {
    if destination.app != source.app || source.app != AppKind::Claude {
        return Err(AppError::Validation(format!(
            "{} isn't a Claude profile",
            destination.label
        )));
    }
    if destination.id == source.id {
        return Err(AppError::Validation(format!(
            "It's already in {}",
            source.label
        )));
    }
    let scans = home_scans(homes);
    let live = live_anywhere(homes, ps_output);
    let owned = owned_by(&source.id, &scans)
        .into_iter()
        .find(|owned| owned.session_id == session_id)
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "session {session_id} not found in {}",
                source.label
            ))
        })?;
    let archive = archive::check_scanned(
        source,
        &scans,
        &live,
        session_id,
        SessionAction::Archive,
        ps_output,
    )?;
    let at_destination = scans.iter().find(|scan| scan.home_id == destination.id);
    let displayed = owned.transcript.as_ref().map(|held| held.summary.clone());
    let cwd = owned
        .record
        .as_ref()
        .and_then(|record| record.cwd.clone())
        .or_else(|| displayed.as_ref().and_then(|summary| summary.cwd.clone()));
    let state = match (&displayed, live.get(session_id)) {
        (None, _) => SessionState::TranscriptMissing,
        (Some(_), Some(LiveHolder::Terminal)) => SessionState::OpenInTerminal,
        _ => SessionState::Idle,
    };
    let mut blockers: Vec<String> = Vec::new();
    for reason in [
        unmovable_reason(source, state, cwd.as_deref()),
        archive.check.blocker.clone(),
    ]
    .into_iter()
    .flatten()
    {
        if !blockers.contains(&reason) {
            blockers.push(reason);
        }
    }
    let destination_records = match at_destination {
        Some(scan) => scan.records.clone(),
        None => read_records(&destination.gui_data_dir),
    };
    let (desktop, record) = desktop_step(
        &owned,
        destination,
        &destination_records,
        displayed.as_ref(),
        cwd.as_deref(),
    );
    let carried = carried_transcripts(&owned, desktop);
    let items = session_items(&carried, homes, destination)?;
    // The destination's copies were read with every home's, so a replaced
    // transcript's last use there is known without reading it again.
    let used_there = |relative: &Path| {
        let path = destination.config_dir.join(relative);
        at_destination?
            .transcripts
            .iter()
            .find(|transcript| transcript.path == path)
            .map(|transcript| transcript.last_used_at)
    };
    let destination_newer = items.iter().any(|item| {
        item.action == ItemAction::Replace
            && item.used_at.is_some_and(|used_at| {
                used_there(&item.relative).is_some_and(|there| there > used_at)
            })
    });
    let memory = memory_merges(carried.iter().copied(), homes);
    let memory_items: Vec<PlannedItem> = memory
        .iter()
        .flat_map(|merge| {
            merge_copies(&merge.from, &destination.config_dir.join(&merge.relative))
                .into_iter()
                .map(|name| PlannedItem {
                    path: merge.relative.join(name).display().to_string(),
                    action: ItemAction::Copy,
                })
        })
        .collect();
    let writes_destination = record.is_some()
        || items.iter().any(|item| item.action != ItemAction::Same)
        || !memory_items.is_empty();
    let mut apps_to_quit: Vec<AppToQuit> = archive.check.app_to_quit.clone().into_iter().collect();
    if writes_destination && desktop_pid(destination, ps_output).is_some() {
        apps_to_quit.push(AppToQuit::of(destination));
    }
    let moving = items
        .iter()
        .filter(|item| item.action != ItemAction::Same)
        .count();
    let (transcripts, others): (Vec<&Item>, Vec<&Item>) =
        items.iter().partition(|item| item.used_at.is_some());
    let planned = |item: &Item| PlannedItem {
        path: item.relative.display().to_string(),
        action: item.action,
    };
    let plan = MovePlan {
        summary: summary(moving, memory_items.len(), source, destination),
        items: others
            .into_iter()
            .map(planned)
            .chain(memory_items)
            .chain(transcripts.into_iter().map(planned))
            .collect(),
        destination_newer,
        desktop,
        blockers,
        apps_to_quit,
        notes: notes(desktop, &owned, record.as_ref(), destination),
    };
    Ok(Prepared {
        plan,
        session_id: session_id.to_string(),
        source: source.clone(),
        destination: destination.clone(),
        items,
        memory,
        record,
        writes_destination,
        archive: archive.target,
    })
}

/// Refuse to write `destination`'s files while its desktop app runs.
pub(super) fn refuse_running(destination: &Home) -> AppResult<()> {
    if running_desktop_pid(destination)?.is_some() {
        return Err(running_again(destination));
    }
    Ok(())
}

/// The transcripts a move of `owned` carries, the one shown last. All it
/// claims go when the destination's desktop app lists the session, by the
/// record the move writes or one it has. Else only the shown one goes: the
/// earlier ones are sessions it already contains, and without a record
/// claiming them each would list there as a session of its own. They stay at
/// the source, claimed by its archived record, so Restore brings back the
/// whole session.
fn carried_transcripts(owned: &Owned, desktop: DesktopAction) -> Vec<&HeldTranscript> {
    let displayed = owned.transcript.as_ref();
    let mut carried: Vec<&HeldTranscript> = Vec::new();
    if matches!(desktop, DesktopAction::Add | DesktopAction::AlreadyListed) {
        let shown = displayed.map(|held| held.summary.session_id.as_str());
        carried.extend(
            owned
                .claimed_transcripts
                .iter()
                .filter(|held| Some(held.summary.session_id.as_str()) != shown),
        );
    }
    carried.extend(displayed);
    carried
}

/// The files and folders the `carried` transcripts are made of, each from
/// the config dir of the home holding it, with what the move does with each
/// at `destination`: every transcript's bundle and the plan files it wrote,
/// sorted, then the transcripts themselves, in the order given.
fn session_items(
    carried: &[&HeldTranscript],
    homes: &[Home],
    destination: &Home,
) -> AppResult<Vec<Item>> {
    let mut others: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut transcripts: Vec<(PathBuf, PathBuf, DateTime<Utc>)> = Vec::new();
    for each in carried {
        let Some(holder) = homes.iter().find(|home| home.id == each.home_id) else {
            continue;
        };
        let config_dir = &holder.config_dir;
        let plans = each
            .summary
            .plan_slugs
            .iter()
            .map(|slug| config_dir.join(PLANS_DIR).join(format!("{slug}.md")))
            .filter(|path| path.is_file());
        for path in bundle_paths(config_dir, &each.summary)
            .into_iter()
            .chain(plans)
        {
            let relative = relative_to(&path, config_dir)?;
            if path == each.summary.path {
                transcripts.push((path, relative, each.summary.last_used_at));
            } else {
                others.push((path, relative));
            }
        }
    }
    others.sort_by(|left, right| left.1.cmp(&right.1));
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    let all = others
        .into_iter()
        .map(|(from, relative)| (from, relative, None))
        .chain(
            transcripts
                .into_iter()
                .map(|(from, relative, used_at)| (from, relative, Some(used_at))),
        );
    for (from, relative, used_at) in all {
        if !seen.insert(relative.clone()) {
            continue;
        }
        let action = compare(&from, &destination.config_dir.join(&relative))?;
        items.push(Item {
            from,
            relative,
            action,
            used_at,
        });
    }
    Ok(items)
}

/// `path` relative to `config_dir`, which holds it.
pub(super) fn relative_to(path: &Path, config_dir: &Path) -> AppResult<PathBuf> {
    path.strip_prefix(config_dir)
        .map(Path::to_path_buf)
        .map_err(|_| {
            AppError::Validation(format!(
                "{} isn't in {}",
                path.display(),
                config_dir.display()
            ))
        })
}

/// The project memory folders of the projects the `held` transcripts are in:
/// `projects/<slug>/memory/` beside each, in the home holding it.
pub(super) fn memory_merges<'a>(
    held: impl IntoIterator<Item = &'a HeldTranscript>,
    homes: &[Home],
) -> Vec<MemoryMerge> {
    let mut merges: Vec<MemoryMerge> = Vec::new();
    for held in held {
        let Some(holder) = homes.iter().find(|home| home.id == held.home_id) else {
            continue;
        };
        let Some(project) = held.summary.path.parent() else {
            continue;
        };
        let Ok(relative) = relative_to(&project.join(MEMORY_DIR), &holder.config_dir) else {
            continue;
        };
        if merges.iter().all(|merge| merge.relative != relative) {
            merges.push(MemoryMerge {
                from: project.join(MEMORY_DIR),
                relative,
            });
        }
    }
    merges
}

/// What the destination's desktop app, with `records`, does about `owned`,
/// shown by `displayed` and working in `cwd`, with the record to write if one
/// is. Only the records of the account the app is signed in to count as
/// listing it: the app shows no other. A session without a transcript or a
/// folder to show can't have a record, so signing in wouldn't list it.
fn desktop_step(
    owned: &Owned,
    destination: &Home,
    records: &[DesktopRecord],
    displayed: Option<&TranscriptSummary>,
    cwd: Option<&str>,
) -> (DesktopAction, Option<RecordWrite>) {
    if !destination.gui_data_dir.is_dir() {
        return (DesktopAction::NoDesktop, None);
    }
    let recordable = displayed.filter(|_| cwd.is_some());
    let Some(account_dir) = current_account_dir(destination) else {
        if recordable.is_none() {
            return (DesktopAction::NoDesktop, None);
        }
        return (DesktopAction::SignInNeeded, None);
    };
    let source_local = owned.record.as_ref().map(|record| record.local_id.as_str());
    let session_id = owned.session_id.as_str();
    let listed = records.iter().any(|record| {
        !record.archived
            && record.path.parent() == Some(account_dir.as_path())
            && (Some(record.local_id.as_str()) == source_local
                || record.cli_session_id.as_deref() == Some(session_id)
                || record
                    .prior_cli_session_ids
                    .iter()
                    .any(|id| id == session_id))
    });
    if listed {
        return (DesktopAction::AlreadyListed, None);
    }
    let Some(displayed) = recordable else {
        return (DesktopAction::NoDesktop, None);
    };
    let priors = owned
        .claimed_transcripts
        .iter()
        .map(|held| held.summary.session_id.clone())
        .filter(|id| *id != displayed.session_id)
        .collect();
    let write = RecordWrite {
        account_dir,
        source: owned.record.clone(),
        displayed: displayed.clone(),
        priors,
    };
    (DesktopAction::Add, Some(write))
}

/// The plan's one line: how many files, `moving`, and memory files,
/// `memory`, move from `source` to `destination`.
fn summary(moving: usize, memory: usize, source: &Home, destination: &Home) -> String {
    let (from, to) = (&source.label, &destination.label);
    match (moving, memory) {
        (0, 0) => format!("{to} has every file already; the session is archived in {from}"),
        (0, memory) => format!("Moves {} from {from} to {to}", files(memory, "memory file")),
        (moving, 0) => format!("Moves {} from {from} to {to}", files(moving, "file")),
        (moving, memory) => format!(
            "Moves {} from {from} to {to}, and {}",
            files(moving, "file"),
            files(memory, "memory file")
        ),
    }
}

/// `count` of `noun`, pluralized: `1 file`, `2 files`.
fn files(count: usize, noun: &str) -> String {
    if count == 1 {
        return format!("1 {noun}");
    }
    format!("{count} {noun}s")
}

/// What the user should know about the move that doesn't stop it: the
/// destination's app needs signing in to list it; a record moving to it
/// leaves its connectors behind, and, moving between accounts, Remote
/// Control on other devices starts afresh.
fn notes(
    desktop: DesktopAction,
    owned: &Owned,
    record: Option<&RecordWrite>,
    destination: &Home,
) -> Vec<String> {
    let mut notes = Vec::new();
    if desktop == DesktopAction::SignInNeeded {
        notes.push(format!(
            "Sign in to Claude in {}'s desktop app to see it there.",
            destination.label
        ));
    }
    if let (Some(source), Some(write)) = (&owned.record, record) {
        notes.push(format!(
            "Connectors and MCP servers come from {}'s settings",
            destination.label
        ));
        let destination_account = write
            .account_dir
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str());
        if record_account(&source.path) != destination_account {
            notes.push(REMOTE_CONTROL_NOTE.to_string());
        }
    }
    notes
}

#[cfg(test)]
mod tests;
