//! Moving a Claude session to another profile of the app.
//!
//! [`plan`] says what a move would do without touching anything; [`execute`]
//! carries out what it planned. A move copies the session's files into the
//! destination's config dir, backing up whatever it replaces, merges the
//! project's memory, copies the transcripts last, adds the session to the
//! destination's desktop app when it is signed in, and finally archives the
//! session at the source, so Restore there undoes it.

use std::collections::{BTreeSet, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::archive::{self, Target};
use super::archive_store::{move_all, occupied, replaced_dir};
use super::copy::{compare, place, ItemAction};
use super::desktop::{
    build_destination_record, current_account_dir, deleted_in, read_records, record_account,
    write_destination_record, DesktopRecord,
};
use super::live::LiveHolder;
use super::memory::{merge_memory, merge_writes};
use super::ownership::{owned_by, HeldTranscript, Owned};
use super::transcript::{bundle_paths, summarize, TranscriptSummary};
use crate::app_kind::AppKind;
use crate::error::{AppError, AppResult};
use crate::sessions::actions::{AppToQuit, SessionAction};
use crate::sessions::instance::{desktop_pid, running_again, running_desktop_pid};
use crate::sessions::list::{home_scans, live_anywhere, unmovable_reason, SessionState};
use crate::sessions::Home;

/// A project's memory folder, beside its transcripts.
const MEMORY_DIR: &str = "memory";

/// The folder under a config dir holding the plans sessions wrote.
pub(super) const PLANS_DIR: &str = "plans";

/// Where, in a move's backup folder, a desktop record it replaced goes.
const RECORDS_BACKUP: &str = "desktop-records";

/// The note for a desktop session moving to another account.
const REMOTE_CONTROL_NOTE: &str =
    "On other devices, Remote Control shows only messages sent after the move.";

/// What a move does about the destination's desktop app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DesktopAction {
    /// A record of the session is written, so the app lists it.
    Add,
    /// The app lists the session already.
    AlreadyListed,
    /// The app isn't signed in, so it has no session list to add to.
    SignInNeeded,
    /// The destination has no desktop app.
    NoDesktop,
}

/// One file or folder a move copies, as the plan shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedItem {
    /// Where it goes, relative to the destination's config dir.
    pub path: String,
    /// What the move does with it.
    pub action: ItemAction,
}

/// What moving a session would do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MovePlan {
    /// One line saying what moves where: `Moves 3 files from Work to Personal`.
    pub summary: String,
    /// The files and folders the move copies, transcripts last.
    pub items: Vec<PlannedItem>,
    /// The destination has a copy of a transcript that differs and was used
    /// more recently: moving would roll it back, so it takes the user's
    /// say-so.
    pub destination_newer: bool,
    /// What the move does about the destination's desktop app.
    pub desktop: DesktopAction,
    /// Why the move can't be done, when only the user can change that.
    pub blockers: Vec<String>,
    /// The desktop apps that have to quit first, at the source, the
    /// destination or both.
    pub apps_to_quit: Vec<AppToQuit>,
    /// Things worth knowing that don't stop the move.
    pub notes: Vec<String>,
}

/// What a move did that the user should hear about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveReport {
    /// The memory files both profiles have, differently; the destination's
    /// were kept.
    pub memory_conflicts: Vec<String>,
}

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

/// The field of a transcript record naming the plan it wrote.
#[derive(Deserialize)]
struct SlugRecord {
    /// The plan's name: `<config>/plans/<slug>.md`.
    slug: Option<String>,
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

/// A planned move, with what [`execute`] needs to carry it out.
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
/// destination's when the move writes its record or files it lists.
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
    let owned = owned_by(&source.id, &home_scans(homes))
        .into_iter()
        .find(|owned| owned.session_id == session_id)
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "session {session_id} not found in {}",
                source.label
            ))
        })?;
    let archive = archive::check(source, homes, session_id, SessionAction::Archive, ps_output)?;
    let displayed = owned.transcript.as_ref().map(|held| held.summary.clone());
    let cwd = owned
        .record
        .as_ref()
        .and_then(|record| record.cwd.clone())
        .or_else(|| displayed.as_ref().and_then(|summary| summary.cwd.clone()));
    let state = match (&displayed, live_anywhere(homes, ps_output).get(session_id)) {
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
    let (desktop, record) = desktop_step(&owned, destination, displayed.as_ref(), cwd.as_deref());
    let carried = carried_transcripts(&owned, desktop);
    let items = session_items(&carried, homes, destination)?;
    let destination_newer = items.iter().any(|item| {
        item.action == ItemAction::Replace
            && item.used_at.is_some_and(|used_at| {
                summarize(&destination.config_dir.join(&item.relative))
                    .is_some_and(|there| there.last_used_at > used_at)
            })
    });
    let memory = memory_merges(carried.iter().copied(), homes);
    let writes_destination = record.is_some()
        || items.iter().any(|item| item.action != ItemAction::Same)
        || memory
            .iter()
            .any(|merge| merge_writes(&merge.from, &destination.config_dir.join(&merge.relative)));
    let mut apps_to_quit: Vec<AppToQuit> = archive.check.app_to_quit.clone().into_iter().collect();
    if writes_destination && desktop_pid(destination, ps_output).is_some() {
        apps_to_quit.push(AppToQuit::of(destination));
    }
    let plan = MovePlan {
        summary: summary(&items, source, destination),
        items: items
            .iter()
            .map(|item| PlannedItem {
                path: item.relative.display().to_string(),
                action: item.action,
            })
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

/// Carry out `prepared`, a move [`plan`] let through, at `at`: copy the
/// files, merge the memory, copy the transcripts, write the destination's
/// desktop record, then archive the session at the source. What is replaced
/// is backed up in the destination's config dir first (see
/// [`replaced_dir`]), and a failure after that says where. A step that fails
/// stops the move there; as the transcripts are copied last, the destination
/// then has no session yet, and the source is untouched.
///
/// The destination's desktop app must not run while anything of its home is
/// written, so one that started again since the check is looked for right
/// before the first write, and the move refused if it runs.
pub fn execute(prepared: Prepared, at: DateTime<Utc>) -> AppResult<MoveReport> {
    let backup = replaced_dir(&prepared.destination.config_dir, &prepared.session_id, at)?;
    if prepared.writes_destination {
        refuse_running(&prepared.destination)?;
    }
    carry_out(prepared, &backup).map_err(|error| {
        if !backup.exists() {
            return error;
        }
        AppError::Validation(format!(
            "{error}. What the move replaced is backed up in {}",
            backup.display()
        ))
    })
}

/// [`execute`] `prepared`, backing up what it replaces into `backup`.
fn carry_out(prepared: Prepared, backup: &Path) -> AppResult<MoveReport> {
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
        copy_item(item, &destination, backup)?;
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
        copy_item(item, &destination, backup)?;
    }
    if let Some(write) = record {
        write_record(&destination, write, backup)?;
    }
    archive::apply(&source, archive, SessionAction::Archive)?;
    Ok(MoveReport { memory_conflicts })
}

/// Refuse to write `destination`'s files while its desktop app runs.
pub(super) fn refuse_running(destination: &Home) -> AppResult<()> {
    if running_desktop_pid(destination)?.is_some() {
        return Err(running_again(destination));
    }
    Ok(())
}

/// Copy `item` into `destination`'s config dir, backing up what it replaces
/// into `backup`. One the destination has the same of is left alone.
fn copy_item(item: &Item, destination: &Home, backup: &Path) -> AppResult<()> {
    if item.action == ItemAction::Same {
        return Ok(());
    }
    place(&item.from, &destination.config_dir, &item.relative, backup)
}

/// Write `destination`'s desktop record of the moved session. Its desktop app
/// writes its records back from memory, so one that started again since the
/// check is looked for right before, and the write refused if it runs. A
/// record already there under the same id (archived there) is backed up into
/// `backup` first; an id the app marks deleted there is swapped for a new
/// one, as the app would never show it.
fn write_record(destination: &Home, write: RecordWrite, backup: &Path) -> AppResult<()> {
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
    if occupied(&write.account_dir.join(&name)) {
        move_all(
            std::slice::from_ref(&name),
            &write.account_dir,
            &backup.join(RECORDS_BACKUP),
            &mut |from, to| fs::rename(from, to),
        )
        .map_err(|failed| failed.error)?;
    }
    write_destination_record(&write.account_dir, &record).map(drop)
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
        let plans = plan_slugs(&each.summary.path)
            .into_iter()
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

/// The plans the transcript at `path` wrote: the `slug` of its records, each
/// the name of a `<config>/plans/<slug>.md`. Only names that stay in that
/// folder count.
pub(super) fn plan_slugs(path: &Path) -> BTreeSet<String> {
    let Ok(file) = File::open(path) else {
        return BTreeSet::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter(|line| line.contains("\"slug\""))
        .filter_map(|line| serde_json::from_str::<SlugRecord>(&line).ok()?.slug)
        .filter(|slug| !slug.is_empty() && !slug.starts_with('.') && !slug.contains('/'))
        .collect()
}

/// What the destination's desktop app does about `owned`, shown by
/// `displayed` and working in `cwd`, with the record to write if one is. Only
/// the records of the account the app is signed in to count as listing it:
/// the app shows no other.
fn desktop_step(
    owned: &Owned,
    destination: &Home,
    displayed: Option<&TranscriptSummary>,
    cwd: Option<&str>,
) -> (DesktopAction, Option<RecordWrite>) {
    if !destination.gui_data_dir.is_dir() {
        return (DesktopAction::NoDesktop, None);
    }
    let Some(account_dir) = current_account_dir(destination) else {
        return (DesktopAction::SignInNeeded, None);
    };
    let source_local = owned.record.as_ref().map(|record| record.local_id.as_str());
    let session_id = owned.session_id.as_str();
    let listed = read_records(&destination.gui_data_dir)
        .iter()
        .any(|record| {
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
    let Some(displayed) = displayed.filter(|_| cwd.is_some()) else {
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

/// The plan's one line: how many files move from `source` to `destination`.
fn summary(items: &[Item], source: &Home, destination: &Home) -> String {
    let moving = items
        .iter()
        .filter(|item| item.action != ItemAction::Same)
        .count();
    match moving {
        0 => format!(
            "{} has every file already; the session is archived in {}",
            destination.label, source.label
        ),
        1 => format!(
            "Moves 1 file from {} to {}",
            source.label, destination.label
        ),
        count => format!(
            "Moves {count} files from {} to {}",
            source.label, destination.label
        ),
    }
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
mod tests {
    use std::fs::{self, File};
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, SystemTime};

    use serde_json::{json, Value};
    use tempfile::tempdir;

    use super::*;
    use crate::app_kind::AppKind;
    use crate::error::AppError;
    use crate::sessions::actions::SessionAction;
    use crate::sessions::claude::archive;
    use crate::sessions::claude::archive_store::archived_bundles;
    use crate::sessions::claude::desktop::read_records;
    use crate::sessions::claude::ownership::owned_by;
    use crate::sessions::list::home_scans;
    use crate::test_support::fake_wrapper_process;

    const WORK_ACCOUNT: &str = "1a19a582-d7b1-4f72-acef-cbe78c1a68e4";
    const WORK_ORG: &str = "18d53058-434e-4c78-9624-e290f7a80ccb";
    const PERSONAL_ACCOUNT: &str = "a99c6b36-dd42-44d7-b3ae-9496265549fd";
    const PERSONAL_ORG: &str = "527aadd2-01c3-49a6-a770-e65e047242c3";

    /// A managed Claude home named `name`, under `root`, whose desktop app
    /// has been opened.
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

    /// Write `contents` to `path`, making its folder.
    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
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

    /// When the file at `path` was last written.
    fn modified(path: &Path) -> SystemTime {
        fs::metadata(path).unwrap().modified().unwrap()
    }

    /// The JSON file at `path`.
    fn read_value(path: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    /// When a transcript's last record was written, in the tests.
    const WRITTEN: u64 = 1_780_000_000;

    /// Gives `home` transcript `session`, last used at `timestamp`, working in
    /// `cwd`, with a subagent transcript beside it and a file history, all
    /// dated [`WRITTEN`]. Returns the transcript's path.
    fn transcript(home: &Home, session: &str, timestamp: &str, cwd: &str) -> PathBuf {
        let dir = home.config_dir.join("projects/-work-app");
        let line = json!({
            "type": "user",
            "sessionId": session,
            "timestamp": timestamp,
            "cwd": cwd,
            "message": { "role": "user", "content": "Fix the login bug" },
        });
        let path = dir.join(format!("{session}.jsonl"));
        let written = SystemTime::UNIX_EPOCH + Duration::from_secs(WRITTEN);
        write(&path, &format!("{line}\n"));
        date(&path, written);
        let subagent = dir.join(session).join("subagents/agent-1.jsonl");
        write(&subagent, "{}\n");
        date(&subagent, written);
        let history = home
            .config_dir
            .join("file-history")
            .join(session)
            .join("abc@v1");
        write(&history, "old");
        date(&history, written);
        path
    }

    /// Signs `home`'s desktop app in to `account`, in `org`.
    fn sign_in(home: &Home, account: &str, org: &str) {
        write(
            &home.gui_data_dir.join("config.json"),
            &json!({ "lastKnownAccountUuid": account }).to_string(),
        );
        write(
            &home.config_dir.join(".claude.json"),
            &json!({ "oauthAccount": { "accountUuid": account, "organizationUuid": org } })
                .to_string(),
        );
        fs::create_dir_all(records_dir(home, account, org)).unwrap();
    }

    /// Where `home`'s desktop app keeps the records of `account`, in `org`.
    fn records_dir(home: &Home, account: &str, org: &str) -> PathBuf {
        home.gui_data_dir
            .join("claude-code-sessions")
            .join(account)
            .join(org)
    }

    /// Writes `home`'s desktop record `local_<uuid>` of `fields`, under
    /// [`WORK_ACCOUNT`]. Returns its path.
    fn record(home: &Home, uuid: &str, fields: Value) -> PathBuf {
        let path = records_dir(home, WORK_ACCOUNT, WORK_ORG).join(format!("local_{uuid}.json"));
        write(&path, &fields.to_string());
        path
    }

    /// A desktop record of session `s`, with fields bound to its account.
    fn desktop_fields() -> Value {
        json!({
            "sessionId": "local_r1",
            "cliSessionId": "s",
            "cwd": "/work/app",
            "title": "Audit the API",
            "model": "claude-opus-5-5",
            "isArchived": false,
            "remoteMcpServersConfig": [{ "uuid": "x" }],
            "sessionPermissionUpdates": [],
            "alwaysAllowedReasons": {},
            "spawnSeed": 7,
        })
    }

    /// Plans moving session `session_id` of `source` to `destination`, then
    /// carries the move out.
    fn move_session(
        source: &Home,
        destination: &Home,
        homes: &[Home],
        session_id: &str,
    ) -> AppResult<MoveReport> {
        let prepared = plan(source, destination, homes, session_id, "")?;
        assert_eq!(prepared.plan.blockers, Vec::<String>::new());
        execute(prepared, "2026-09-23T08:15:00Z".parse().unwrap())
    }

    /// The paths and actions a plan lists.
    fn planned(plan: &MovePlan) -> Vec<(&str, ItemAction)> {
        plan.items
            .iter()
            .map(|item| (item.path.as_str(), item.action))
            .collect()
    }

    #[test]
    fn the_plan_copies_what_is_missing_leaves_what_is_the_same_and_replaces_the_rest() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        write(&personal.config_dir.join("file-history/s/abc@v1"), "old");
        write(
            &personal
                .config_dir
                .join("projects/-work-app/s/subagents/agent-1.jsonl"),
            "{\"other\":1}\n",
        );
        let homes = [work.clone(), personal.clone()];

        let prepared = plan(&work, &personal, &homes, "s", "").unwrap();

        assert_eq!(
            planned(&prepared.plan),
            [
                ("file-history/s", ItemAction::Same),
                ("projects/-work-app/s", ItemAction::Replace),
                ("projects/-work-app/s.jsonl", ItemAction::Copy),
            ]
        );
        assert_eq!(prepared.plan.summary, "Moves 2 files from Work to Personal");
        assert!(!prepared.plan.destination_newer);
        assert_eq!(prepared.plan.blockers, Vec::<String>::new());
        assert_eq!(prepared.plan.apps_to_quit, []);
    }

    #[test]
    fn a_newer_copy_at_the_destination_is_flagged() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        transcript(&personal, "s", "2026-09-02T10:00:00Z", "/work/app");
        let homes = [work.clone(), personal.clone()];

        let newer = plan(&work, &personal, &homes, "s", "").unwrap().plan;
        let older = plan(&personal, &work, &homes, "s", "").unwrap().plan;

        assert!(newer.destination_newer);
        assert!(!older.destination_newer);
    }

    #[test]
    fn a_moved_cli_session_keeps_its_dates_and_is_archived_at_the_source() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        let source = transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        let homes = [work.clone(), personal.clone()];

        move_session(&work, &personal, &homes, "s").unwrap();

        let copied = personal.config_dir.join("projects/-work-app/s.jsonl");
        assert_eq!(
            modified(&copied),
            SystemTime::UNIX_EPOCH + Duration::from_secs(WRITTEN)
        );
        assert!(personal
            .config_dir
            .join("projects/-work-app/s/subagents/agent-1.jsonl")
            .exists());
        assert!(personal.config_dir.join("file-history/s/abc@v1").exists());
        assert!(!source.exists());
        let archived = archived_bundles(&work.config_dir);
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0].session_id, "s");
    }

    #[test]
    fn a_move_cut_short_leaves_no_transcript_at_the_destination() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        let source = transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        let history = personal.config_dir.join("file-history");
        fs::create_dir_all(&history).unwrap();
        fs::set_permissions(&history, fs::Permissions::from_mode(0o555)).unwrap();
        let homes = [work.clone(), personal.clone()];

        let moved = move_session(&work, &personal, &homes, "s");

        fs::set_permissions(&history, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(moved.is_err());
        assert!(!personal
            .config_dir
            .join("projects/-work-app/s.jsonl")
            .exists());
        assert!(source.exists());
        assert_eq!(archived_bundles(&work.config_dir), []);
    }

    #[test]
    fn a_desktop_session_is_listed_under_the_destinations_account_without_the_sources_bindings() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        sign_in(&work, WORK_ACCOUNT, WORK_ORG);
        sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
        transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        let source_record = record(&work, "r1", desktop_fields());
        let homes = [work.clone(), personal.clone()];

        let prepared = plan(&work, &personal, &homes, "s", "").unwrap();

        assert_eq!(prepared.plan.desktop, DesktopAction::Add);
        assert_eq!(
            prepared.plan.notes,
            [
                "Connectors and MCP servers come from Personal's settings",
                "On other devices, Remote Control shows only messages sent after the move.",
            ]
        );

        execute(prepared, "2026-09-23T08:15:00Z".parse().unwrap()).unwrap();

        let written = records_dir(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG).join("local_r1.json");
        assert_eq!(
            read_value(&written),
            json!({
                "sessionId": "local_r1",
                "cliSessionId": "s",
                "priorCliSessionIds": [],
                "cwd": "/work/app",
                "title": "Audit the API",
                "model": "claude-opus-5-5",
                "isArchived": false,
            })
        );
        assert_eq!(read_value(&source_record)["isArchived"], json!(true));
        assert!(work.config_dir.join("projects/-work-app/s.jsonl").exists());
    }

    #[test]
    fn a_cli_session_gets_a_record_only_where_the_desktop_app_is_signed_in() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        let side = home(root.path(), "Side");
        sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
        transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        let homes = [work.clone(), personal.clone(), side.clone()];

        let unsigned = plan(&work, &side, &homes, "s", "").unwrap().plan;

        assert_eq!(unsigned.desktop, DesktopAction::SignInNeeded);
        assert_eq!(
            unsigned.notes,
            ["Sign in to Claude in Side's desktop app to see it there."]
        );

        move_session(&work, &personal, &homes, "s").unwrap();

        let records = read_records(&personal.gui_data_dir);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].cli_session_id.as_deref(), Some("s"));
        assert_eq!(records[0].cwd.as_deref(), Some("/work/app"));
        assert!(!records[0].archived);
        assert!(records[0].path.starts_with(records_dir(
            &personal,
            PERSONAL_ACCOUNT,
            PERSONAL_ORG
        )));
    }

    #[test]
    fn a_destination_without_a_desktop_app_gets_the_cli_half_only() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        fs::remove_dir(&personal.gui_data_dir).unwrap();
        transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        let homes = [work.clone(), personal.clone()];

        let prepared = plan(&work, &personal, &homes, "s", "").unwrap();

        assert_eq!(prepared.plan.desktop, DesktopAction::NoDesktop);
        assert_eq!(prepared.plan.notes, Vec::<String>::new());
    }

    #[test]
    fn every_transcript_a_record_claims_moves_from_wherever_it_is() {
        let root = tempdir().unwrap();
        let default = home(root.path(), "Default");
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
        transcript(&work, "s", "2026-09-02T10:00:00Z", "/work/app");
        transcript(&default, "earlier", "2026-09-01T10:00:00Z", "/work/app");
        record(
            &work,
            "r1",
            json!({ "cliSessionId": "s", "priorCliSessionIds": ["earlier"] }),
        );
        let homes = [default.clone(), work.clone(), personal.clone()];

        let prepared = plan(&work, &personal, &homes, "s", "").unwrap();

        assert_eq!(
            planned(&prepared.plan),
            [
                ("file-history/earlier", ItemAction::Copy),
                ("file-history/s", ItemAction::Copy),
                ("projects/-work-app/earlier", ItemAction::Copy),
                ("projects/-work-app/s", ItemAction::Copy),
                ("projects/-work-app/earlier.jsonl", ItemAction::Copy),
                ("projects/-work-app/s.jsonl", ItemAction::Copy),
            ]
        );

        execute(prepared, "2026-09-23T08:15:00Z".parse().unwrap()).unwrap();

        assert!(personal
            .config_dir
            .join("projects/-work-app/earlier.jsonl")
            .exists());
        assert!(default
            .config_dir
            .join("projects/-work-app/earlier.jsonl")
            .exists());
    }

    #[test]
    fn a_plan_file_the_session_wrote_moves_with_it() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        let path = transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        let lines = format!(
            "{}{}\n{}\n",
            fs::read_to_string(&path).unwrap(),
            json!({ "type": "assistant", "timestamp": "2026-09-01T10:01:00Z", "slug": "bold-plan" }),
            json!({ "type": "assistant", "timestamp": "2026-09-01T10:02:00Z", "slug": "../escape" }),
        );
        write(&path, &lines);
        write(&work.config_dir.join("plans/bold-plan.md"), "# Plan");
        let homes = [work.clone(), personal.clone()];

        move_session(&work, &personal, &homes, "s").unwrap();

        assert_eq!(
            fs::read_to_string(personal.config_dir.join("plans/bold-plan.md")).unwrap(),
            "# Plan"
        );
    }

    #[test]
    fn project_memory_is_merged_and_its_conflicts_reported() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        let memory = "projects/-work-app/memory";
        write(&work.config_dir.join(memory).join("style.md"), "Tabs");
        write(&work.config_dir.join(memory).join("deploy.md"), "Vercel");
        write(
            &personal.config_dir.join(memory).join("deploy.md"),
            "Netlify",
        );
        let homes = [work.clone(), personal.clone()];

        let report = move_session(&work, &personal, &homes, "s").unwrap();

        assert_eq!(report.memory_conflicts, ["deploy.md"]);
        assert!(personal.config_dir.join(memory).join("style.md").exists());
    }

    #[test]
    fn what_stops_a_move_is_listed() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        transcript(&work, "cli", "2026-09-01T10:00:00Z", "/work/app");
        let scratch = work.gui_data_dir.join("local-agent-mode-sessions/x");
        record(
            &work,
            "r1",
            json!({ "cliSessionId": "scratch", "cwd": scratch.display().to_string() }),
        );
        transcript(&work, "scratch", "2026-09-01T10:00:00Z", "/tmp");
        record(&work, "r2", json!({ "cliSessionId": "gone" }));
        write(
            &work.config_dir.join("sessions/4100.json"),
            &json!({ "pid": 4100, "sessionId": "cli", "entrypoint": "cli" }).to_string(),
        );
        let homes = [work.clone(), personal.clone()];
        let blockers = |session_id| {
            plan(&work, &personal, &homes, session_id, "  4100 claude\n")
                .unwrap()
                .plan
                .blockers
        };

        assert_eq!(
            blockers("scratch"),
            ["Lives in the desktop app's scratch folder"]
        );
        assert_eq!(blockers("gone"), ["Transcript deleted"]);
        assert_eq!(blockers("cli"), ["Close it in the terminal first"]);
    }

    #[test]
    fn a_session_moves_only_to_another_profile_of_the_same_app() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let mut codex = home(root.path(), "Codex");
        codex.app = AppKind::Codex;
        transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        let homes = [work.clone()];

        assert!(matches!(
            plan(&work, &work, &homes, "s", ""),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            plan(&work, &codex, &homes, "s", ""),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn the_desktop_apps_in_the_way_are_named() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
        transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        record(&work, "r1", desktop_fields());
        let homes = [work.clone(), personal.clone()];
        let ps_output = format!(
            "  900 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={}\n  \
             901 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={}\n",
            work.gui_data_dir.display(),
            personal.gui_data_dir.display()
        );

        let apps = plan(&work, &personal, &homes, "s", &ps_output)
            .unwrap()
            .plan
            .apps_to_quit;

        let labels: Vec<&str> = apps.iter().map(|app| app.label.as_str()).collect();
        assert_eq!(labels, ["Claude (Work)", "Claude (Personal)"]);
    }

    #[test]
    fn moving_again_after_restoring_at_the_source_changes_nothing_at_the_destination() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        sign_in(&work, WORK_ACCOUNT, WORK_ORG);
        sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
        transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        let source_record = record(&work, "r1", desktop_fields());
        let homes = [work.clone(), personal.clone()];
        move_session(&work, &personal, &homes, "s").unwrap();
        let restore = archive::check(&work, &homes, "s", SessionAction::Restore, "").unwrap();
        archive::apply(&work, restore.target, SessionAction::Restore).unwrap();
        let dest_dir = records_dir(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
        let first = read_value(&dest_dir.join("local_r1.json"));

        let prepared = plan(&work, &personal, &homes, "s", "").unwrap();

        assert!(planned(&prepared.plan)
            .iter()
            .all(|(_, action)| *action == ItemAction::Same));
        assert_eq!(prepared.plan.desktop, DesktopAction::AlreadyListed);

        execute(prepared, "2026-09-24T08:15:00Z".parse().unwrap()).unwrap();

        assert_eq!(fs::read_dir(&dest_dir).unwrap().count(), 1);
        assert_eq!(read_value(&dest_dir.join("local_r1.json")), first);
        assert!(!personal
            .config_dir
            .join("ai-profiles-archive/.replaced")
            .exists());
        assert_eq!(read_value(&source_record)["isArchived"], json!(true));
    }

    #[test]
    fn what_a_move_replaces_is_backed_up() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        transcript(&work, "s", "2026-09-02T10:00:00Z", "/work/app");
        transcript(&personal, "s", "2026-09-01T10:00:00Z", "/elsewhere");
        let homes = [work.clone(), personal.clone()];

        move_session(&work, &personal, &homes, "s").unwrap();

        let backup = personal
            .config_dir
            .join("ai-profiles-archive/.replaced/s/2026-09-23T08-15-00.000Z");
        assert!(
            fs::read_to_string(backup.join("projects/-work-app/s.jsonl"))
                .unwrap()
                .contains("/elsewhere")
        );
        assert!(
            fs::read_to_string(personal.config_dir.join("projects/-work-app/s.jsonl"))
                .unwrap()
                .contains("/work/app")
        );
    }

    /// Whether `home`, one of `homes`, lists session `session_id`, and the
    /// record it lists it by.
    fn listed(home: &Home, homes: &[Home], session_id: &str) -> Option<Option<DesktopRecord>> {
        owned_by(&home.id, &home_scans(homes))
            .into_iter()
            .find(|owned| owned.session_id == session_id)
            .map(|owned| owned.record)
    }

    /// Restores session `s` at `home`, one of `homes`.
    fn restore(home: &Home, homes: &[Home]) {
        let checked = archive::check(home, homes, "s", SessionAction::Restore, "").unwrap();
        assert_eq!(checked.check.blocker, None);
        archive::apply(home, checked.target, SessionAction::Restore).unwrap();
    }

    #[test]
    fn a_cli_session_restored_at_the_source_lists_there_and_at_the_destination() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
        transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        let homes = [work.clone(), personal.clone()];
        move_session(&work, &personal, &homes, "s").unwrap();

        assert_eq!(listed(&work, &homes, "s"), None);

        restore(&work, &homes);

        assert_eq!(listed(&work, &homes, "s"), Some(None));
        assert!(listed(&personal, &homes, "s").is_some_and(|record| record.is_some()));
    }

    #[test]
    fn a_desktop_session_restored_at_the_source_lists_in_both_by_their_own_records() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        sign_in(&work, WORK_ACCOUNT, WORK_ORG);
        sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
        transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        record(&work, "r1", desktop_fields());
        let homes = [work.clone(), personal.clone()];
        move_session(&work, &personal, &homes, "s").unwrap();

        restore(&work, &homes);

        let at_work = listed(&work, &homes, "s").flatten().unwrap();
        let at_personal = listed(&personal, &homes, "s").flatten().unwrap();
        assert!(!at_work.archived);
        assert!(at_work.path.starts_with(&work.gui_data_dir));
        assert!(!at_personal.archived);
        assert!(at_personal.path.starts_with(&personal.gui_data_dir));
    }

    #[test]
    fn a_session_moved_back_is_active_again_where_it_started() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        let homes = [work.clone(), personal.clone()];
        move_session(&work, &personal, &homes, "s").unwrap();

        let back = plan(&personal, &work, &homes, "s", "").unwrap();
        execute(back, "2026-09-24T08:15:00Z".parse().unwrap()).unwrap();

        assert_eq!(listed(&work, &homes, "s"), Some(None));
        assert_eq!(listed(&personal, &homes, "s"), None);
        assert_eq!(archived_bundles(&personal.config_dir).len(), 1);
    }

    /// The ids of the sessions `home`, one of `homes`, lists.
    fn listed_ids(home: &Home, homes: &[Home]) -> Vec<String> {
        owned_by(&home.id, &home_scans(homes))
            .into_iter()
            .map(|owned| owned.session_id)
            .collect()
    }

    #[test]
    fn where_no_desktop_record_is_written_only_the_shown_transcript_moves() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        let side = home(root.path(), "Side");
        fs::remove_dir(&side.gui_data_dir).unwrap();
        sign_in(&work, WORK_ACCOUNT, WORK_ORG);
        transcript(&work, "s", "2026-09-02T10:00:00Z", "/work/app");
        transcript(&work, "earlier", "2026-09-01T10:00:00Z", "/work/app");
        let mut fields = desktop_fields();
        fields["priorCliSessionIds"] = json!(["earlier"]);
        let source_record = record(&work, "r1", fields);
        let homes = [work.clone(), personal.clone(), side.clone()];

        let to_side = plan(&work, &side, &homes, "s", "").unwrap().plan;
        let prepared = plan(&work, &personal, &homes, "s", "").unwrap();

        assert_eq!(to_side.desktop, DesktopAction::NoDesktop);
        assert_eq!(prepared.plan.desktop, DesktopAction::SignInNeeded);
        let only_shown = [
            ("file-history/s", ItemAction::Copy),
            ("projects/-work-app/s", ItemAction::Copy),
            ("projects/-work-app/s.jsonl", ItemAction::Copy),
        ];
        assert_eq!(planned(&to_side), only_shown);
        assert_eq!(planned(&prepared.plan), only_shown);

        execute(prepared, "2026-09-23T08:15:00Z".parse().unwrap()).unwrap();

        assert_eq!(listed_ids(&personal, &homes), ["s"]);
        assert!(!personal
            .config_dir
            .join("projects/-work-app/earlier.jsonl")
            .exists());
        assert_eq!(read_value(&source_record)["isArchived"], json!(true));

        restore(&work, &homes);

        let restored = owned_by(&work.id, &home_scans(&homes))
            .into_iter()
            .find(|owned| owned.session_id == "s")
            .unwrap();
        let lineage: Vec<(&str, &str)> = restored
            .claimed_transcripts
            .iter()
            .map(|held| (held.summary.session_id.as_str(), held.home_id.as_str()))
            .collect();
        assert_eq!(lineage, [("s", "work"), ("earlier", "work")]);
        assert!(restored.record.is_some_and(|record| !record.archived));
    }

    #[test]
    fn only_records_of_the_account_the_destination_is_signed_in_to_list_it_there() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        // Moved here once, under the account the app was signed in to then.
        transcript(&personal, "s", "2026-09-01T10:00:00Z", "/work/app");
        let old = records_dir(&personal, WORK_ACCOUNT, WORK_ORG).join("local_old.json");
        write(&old, &json!({ "cliSessionId": "s" }).to_string());
        let homes = [work.clone(), personal.clone()];

        let unsigned = plan(&work, &personal, &homes, "s", "").unwrap().plan;

        assert_eq!(unsigned.desktop, DesktopAction::SignInNeeded);

        sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
        let signed = plan(&work, &personal, &homes, "s", "").unwrap().plan;

        assert_eq!(signed.desktop, DesktopAction::Add);
    }

    #[test]
    fn a_running_destination_quits_whenever_the_move_writes_there() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        let homes = [work.clone(), personal.clone()];
        let ps_output = format!(
            "  901 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={}\n",
            personal.gui_data_dir.display()
        );

        let plan = plan(&work, &personal, &homes, "s", &ps_output)
            .unwrap()
            .plan;

        assert_eq!(plan.desktop, DesktopAction::SignInNeeded);
        let labels: Vec<&str> = plan
            .apps_to_quit
            .iter()
            .map(|app| app.label.as_str())
            .collect();
        assert_eq!(labels, ["Claude (Personal)"]);
    }

    #[test]
    fn nothing_is_written_when_the_destination_app_started_again_since_the_check() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
        let source = transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        let homes = [work.clone(), personal.clone()];
        let prepared = plan(&work, &personal, &homes, "s", "").unwrap();
        let mut relaunched = fake_wrapper_process(&root.path().join("app"), &personal.gui_data_dir);

        let moved = execute(prepared, "2026-09-23T08:15:00Z".parse().unwrap());

        relaunched.kill().unwrap();
        relaunched.wait().unwrap();
        assert!(
            matches!(&moved, Err(AppError::Validation(message)) if message == "Claude (Personal) is running again — quit it and try again"),
            "{moved:?}"
        );
        assert!(!personal.config_dir.join("projects").exists());
        assert_eq!(read_records(&personal.gui_data_dir), []);
        assert!(source.exists());
    }

    #[test]
    fn a_move_cut_short_after_replacing_says_where_the_backup_is() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let personal = home(root.path(), "Personal");
        transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
        write(&personal.config_dir.join("file-history/s/abc@v1"), "other");
        let project = personal.config_dir.join("projects/-work-app");
        fs::create_dir_all(&project).unwrap();
        fs::set_permissions(&project, fs::Permissions::from_mode(0o555)).unwrap();
        let homes = [work.clone(), personal.clone()];

        let moved = move_session(&work, &personal, &homes, "s");

        fs::set_permissions(&project, fs::Permissions::from_mode(0o755)).unwrap();
        let message = match &moved {
            Err(AppError::Validation(message)) => message.clone(),
            other => panic!("{other:?}"),
        };
        assert!(
            message.contains("ai-profiles-archive/.replaced/s/2026-09-23T08-15-00.000Z"),
            "{message}"
        );
        assert!(!project.join("s.jsonl").exists());
    }
}
