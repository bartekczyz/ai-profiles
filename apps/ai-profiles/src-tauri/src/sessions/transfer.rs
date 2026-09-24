//! Moving a session from one profile to another.
//!
//! [`plan`] says what a move would do without touching anything; [`transfer`]
//! plans again and carries it out. Nothing in the destination is overwritten
//! without a copy of it going to `<config>/session-transfer-backups/<id>/<time>/`
//! first, every file lands under a temporary name before being renamed into
//! place, and the transcript is copied last, so a move cut short leaves the
//! destination without a session rather than with half of one.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use ai_profiles_core::api::TransferMemoryFile;
use ai_profiles_core::child::run_within;
use ai_profiles_core::memory::{self, Decision, MemoryAction, Side};
use serde::{Deserialize, Serialize};

use super::archive::{archive_files, move_into};
use super::desktop::{self, DesktopRecord, NewRecord};
use super::scan::{self, is_safe_name, TranscriptInfo};
use super::{
    apps_blocker, home, open_in, parse_process_list, push_app, quit_apps, terminal_blocker,
    AppToQuit, Home, OpenIn,
};
use crate::error::{AppError, AppResult};
use crate::launch::process_list;

use super::archive::BACKUPS_DIR;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequest {
    pub source_id: String,
    pub session_id: String,
    pub destination_id: String,
    /// Also list the session in the destination's desktop app.
    pub add_to_desktop: bool,
    /// Take the session out of the source afterwards, so only one copy goes on.
    pub archive_source: bool,
    /// Delete the source's copy afterwards instead, once everything the move
    /// carried is checked to be identical in the destination: frees its
    /// space, and can't be undone. Not with `archive_source`.
    #[serde(default)]
    pub delete_source: bool,
    /// Go ahead even though the destination's copy is newer than the source's.
    #[serde(default)]
    pub replace_newer: bool,
    /// Quit the apps the plan lists in `apps_to_quit` first.
    #[serde(default)]
    pub quit_apps: bool,
    /// What to keep of each memory note both profiles changed, by its path in
    /// the memory folder: every conflict the plan lists needs one.
    #[serde(default)]
    pub memory: HashMap<String, Decision>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemAction {
    /// Not in the destination yet.
    Copy,
    /// Already in the destination, identical.
    Same,
    /// In the destination, different: backed up, then replaced.
    Replace,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    /// Relative to the destination's config dir.
    pub path: String,
    pub action: ItemAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DesktopAction {
    /// Not asked for.
    Skip,
    /// A record will be written.
    Add,
    /// The destination's app already lists the session.
    AlreadyListed,
    /// Asked for, but it can't be done; `desktop_reason` says why.
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferPlan {
    pub session_id: String,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub source_label: String,
    pub destination_label: String,
    pub items: Vec<PlanItem>,
    /// The destination's transcript differs and was written more recently: a
    /// move would roll the conversation back there.
    pub destination_newer: bool,
    pub desktop: DesktopAction,
    pub desktop_reason: Option<String>,
    /// Reasons the move can't happen right now that only the user can clear.
    pub blockers: Vec<String>,
    /// Desktop apps that have to quit first: they hold the session open, or
    /// keep a session list the move changes. ai-profiles can quit them.
    pub apps_to_quit: Vec<AppToQuit>,
    /// Things worth knowing that don't stop the move.
    pub notes: Vec<String>,
    /// What deleting the source's copy afterwards (`delete_source`) frees,
    /// in bytes.
    pub source_bytes: u64,
    /// What archiving the source's copy afterwards takes, in bytes, before
    /// compression: the transcript and the desktop app's records, which are
    /// all an archive holds.
    pub archive_bytes: u64,
    /// The project's memory, file by file: what the move does with each, and
    /// the notes both profiles changed, for the user to decide.
    pub memory: Vec<TransferMemoryFile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferReport {
    pub destination_transcript: String,
    /// Where replaced destination files went, if any were replaced.
    pub backup_dir: Option<String>,
    /// The record written for the destination's desktop app.
    pub desktop_record: Option<String>,
    /// Where the source's transcript (and desktop record) went.
    pub archived_to: Option<String>,
    /// What deleting the source's copy freed, in bytes, when asked for.
    pub freed_bytes: Option<u64>,
    /// Why the source's copy was kept although deleting it was asked for:
    /// the move itself is done.
    pub delete_error: Option<String>,
    /// What merging the project's memory did, one line per file.
    pub memory: Vec<String>,
}

struct Item {
    from: PathBuf,
    to: PathBuf,
    rel: PathBuf,
    action: ItemAction,
}

struct Prepared {
    plan: TransferPlan,
    source: Home,
    destination: Home,
    info: TranscriptInfo,
    items: Vec<Item>,
    source_transcript: PathBuf,
    source_project: String,
    source_records: Vec<DesktopRecord>,
    destination_records_dir: Option<PathBuf>,
    source_memory: PathBuf,
    destination_memory: PathBuf,
    memory_base: PathBuf,
}

/// What moving the session would do, without doing any of it.
pub fn plan(request: &TransferRequest) -> AppResult<TransferPlan> {
    Ok(prepare(request)?.plan)
}

/// Move the session. Refuses whatever [`plan`] lists as a blocker, and a
/// newer destination copy unless `replace_newer` is set. Apps the plan lists
/// to quit are quit first when `quit_apps` is set, and refused otherwise.
pub fn transfer(
    request: &TransferRequest,
    progress: &dyn Fn(&TransferProgress),
) -> AppResult<TransferReport> {
    if request.archive_source && request.delete_source {
        return Err(AppError::Validation(
            "Archive the copy left behind, or delete it: not both.".into(),
        ));
    }
    let afterwards = if request.delete_source {
        Afterwards::Delete
    } else if request.archive_source {
        Afterwards::Archive
    } else {
        Afterwards::Keep
    };
    let mut prepared = prepare(request)?;
    let quitting = prepared.plan.blockers.is_empty()
        && !prepared.plan.apps_to_quit.is_empty()
        && request.quit_apps;
    let steps = Steps::of(&prepared.plan, quitting, afterwards, progress);
    if quitting {
        steps.at(Step::Quit);
        quit_apps(&prepared.plan.apps_to_quit)?;
        prepared = prepare(request)?;
    }
    let plan = &prepared.plan;
    if !plan.blockers.is_empty() {
        return Err(AppError::Validation(plan.blockers.join(" ")));
    }
    if !plan.apps_to_quit.is_empty() {
        return Err(apps_blocker(&plan.apps_to_quit));
    }
    if plan.destination_newer && !request.replace_newer {
        return Err(AppError::Validation(format!(
            "{} has a newer copy of this session. Moving it would roll that copy back.",
            plan.destination_label
        )));
    }
    // Before anything is copied: an undecided note would stop the move half
    // way.
    if let Some(undecided) = first_undecided(plan, &request.memory) {
        return Err(AppError::Validation(format!(
            "memory/{} differs, and both profiles changed it: say which to keep.",
            undecided.path
        )));
    }
    execute(&prepared, &request.memory, afterwards, &steps)
}

/// How far a move has got, as the dialog shows it while it runs: its steps,
/// in order, and the one it's on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub session_id: String,
    pub steps: Vec<String>,
    /// An index into `steps`.
    pub current: usize,
}

/// One step of a move that the dialog names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Quit,
    Copy,
    Desktop,
    Afterwards,
}

/// A move's steps, said as each one starts.
struct Steps<'a> {
    session_id: String,
    named: Vec<(Step, String)>,
    report: &'a dyn Fn(&TransferProgress),
}

impl<'a> Steps<'a> {
    /// The steps moving by `plan` takes: quitting apps first when it does,
    /// adding a desktop record when there's one to add, and what becomes of
    /// the source's copy unless it's kept.
    fn of(
        plan: &TransferPlan,
        quitting: bool,
        afterwards: Afterwards,
        report: &'a dyn Fn(&TransferProgress),
    ) -> Steps<'a> {
        let mut named = Vec::new();
        if quitting {
            let apps: Vec<String> = plan
                .apps_to_quit
                .iter()
                .map(|app| format!("Claude ({})", app.label))
                .collect();
            named.push((Step::Quit, format!("Quitting {}", apps.join(" and "))));
        }
        named.push((
            Step::Copy,
            format!("Copying it to {}", plan.destination_label),
        ));
        if plan.desktop == DesktopAction::Add {
            named.push((
                Step::Desktop,
                format!("Adding it to {}'s desktop app", plan.destination_label),
            ));
        }
        match afterwards {
            Afterwards::Archive => named.push((
                Step::Afterwards,
                format!("Archiving the copy in {}", plan.source_label),
            )),
            Afterwards::Delete => named.push((
                Step::Afterwards,
                format!("Deleting the copy in {}", plan.source_label),
            )),
            Afterwards::Keep => {}
        }
        Steps {
            session_id: plan.session_id.clone(),
            named,
            report,
        }
    }

    /// Say that `step` has started. One the move doesn't take says nothing.
    fn at(&self, step: Step) {
        if let Some(current) = self.named.iter().position(|(named, _)| *named == step) {
            (self.report)(&TransferProgress {
                session_id: self.session_id.clone(),
                steps: self.named.iter().map(|(_, name)| name.clone()).collect(),
                current,
            });
        }
    }
}

/// What becomes of the source's copy once the session has moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Afterwards {
    Keep,
    /// Its transcript and desktop record go to its backups, and can be put
    /// back.
    Archive,
    /// Deleted, once the moved copy is checked to be identical.
    Delete,
}

/// Whether `item` is the session's own, so its source copy can go once the
/// session has moved: everything but plan files, which other sessions may
/// name too.
fn session_own(item: &Item) -> bool {
    !item.rel.starts_with("plans")
}

/// What a file, folder or link takes, as the sum of its files' lengths.
pub(super) fn size_of(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    fs::read_dir(path)
        .map(|entries| entries.flatten().map(|entry| size_of(&entry.path())).sum())
        .unwrap_or(0)
}

/// Delete the source's copy of the session: its own items, once every one of
/// them is checked to be identical in the destination (nothing is deleted
/// unless all are), and the source desktop app's records of it. Returns what
/// it freed.
fn delete_source(items: &[Item], records: &[DesktopRecord]) -> AppResult<u64> {
    let own: Vec<&Item> = items.iter().filter(|item| session_own(item)).collect();
    for item in &own {
        if fs::symlink_metadata(&item.to).is_err() || !identical(&item.from, &item.to)? {
            return Err(AppError::Validation(format!(
                "{} isn't the same as the moved copy",
                item.rel.display()
            )));
        }
    }
    let mut freed = 0;
    for path in own
        .iter()
        .map(|item| &item.from)
        .chain(records.iter().map(|record| &record.path))
    {
        freed += size_of(path);
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.is_dir() => fs::remove_dir_all(path)?,
            Ok(_) => fs::remove_file(path)?,
            Err(_) => {}
        }
    }
    Ok(freed)
}

fn prepare(request: &TransferRequest) -> AppResult<Prepared> {
    let id = request.session_id.as_str();
    if !is_safe_name(id) {
        return Err(AppError::Validation(format!("invalid session id {id:?}")));
    }
    if request.source_id == request.destination_id {
        return Err(AppError::Validation(
            "the session is already in that profile".to_string(),
        ));
    }
    let source = home(&request.source_id)?;
    let destination = home(&request.destination_id)?;

    let (source_project, source_transcript) = single_transcript(&source, id)?
        .ok_or_else(|| AppError::NotFound(format!("session {id} not found in {}", source.label)))?;
    let info = scan::read_transcript(&source_transcript)?;
    let destination_project = match single_transcript(&destination, id)? {
        Some((project, _)) => project,
        None => source_project.clone(),
    };

    let items = items(
        &source,
        &destination,
        id,
        &source_project,
        &destination_project,
        &info,
    )?;
    let transcript = items.last().expect("the transcript is always an item");
    let destination_newer = transcript.action == ItemAction::Replace
        && modified(&transcript.to) > modified(&transcript.from);

    let processes = parse_process_list(&process_list()?);
    let mut blockers = Vec::new();
    if let Some(reason) = scan::unmovable_reason(&source, info.cwd.as_deref()) {
        blockers.push(reason);
    }
    let mut apps_to_quit = Vec::new();
    for side in [&source, &destination] {
        match open_in(side, id, &processes) {
            Some(OpenIn::Desktop) => push_app(&mut apps_to_quit, side),
            Some(OpenIn::Terminal) => blockers.push(terminal_blocker(side)),
            None => {}
        }
    }

    let mut desktop_reason = None;
    let mut destination_records_dir = None;
    let desktop_action = if !request.add_to_desktop {
        DesktopAction::Skip
    } else if !destination.desktop {
        desktop_reason = Some(format!("{} has no desktop app.", destination.label));
        DesktopAction::Unavailable
    } else if desktop::records(&destination.gui_data_dir)
        .iter()
        .any(|record| record.cli_session_id == id && !record.archived)
    {
        DesktopAction::AlreadyListed
    } else if info.cwd.is_none() {
        desktop_reason = Some("The transcript doesn't say which folder it worked in.".to_string());
        DesktopAction::Unavailable
    } else if let Some(dir) = desktop::records_dir(&destination) {
        destination_records_dir = Some(dir);
        if desktop::app_running(&destination, &processes) {
            push_app(&mut apps_to_quit, &destination);
        }
        DesktopAction::Add
    } else {
        desktop_reason = Some(format!(
            "Open Claude ({}) and sign in once, so it has a session list to add to.",
            destination.label
        ));
        DesktopAction::Unavailable
    };

    let source_records: Vec<DesktopRecord> = desktop::records(&source.gui_data_dir)
        .into_iter()
        .filter(|record| record.cli_session_id == id)
        .collect();
    if (request.archive_source || request.delete_source)
        && !source_records.is_empty()
        && desktop::app_running(&source, &processes)
    {
        push_app(&mut apps_to_quit, &source);
    }

    let mut notes = vec![format!(
        "Connectors, MCP servers, plugins and permissions come from {}'s own settings.",
        destination.label
    )];
    if let (Some(from), Some(to)) = (
        desktop::records_dir(&source),
        desktop::records_dir(&destination),
    ) {
        if from.parent() != to.parent() {
            notes.push(
                "The accounts differ: Remote Control on another device shows only messages sent after the move."
                    .to_string(),
            );
        }
    }

    let source_memory = source
        .config_dir
        .join("projects")
        .join(&source_project)
        .join("memory");
    let destination_memory = destination
        .config_dir
        .join("projects")
        .join(&destination_project)
        .join("memory");
    let memory_base = memory_base(&source_project)?;
    let memory_plan = if source_memory.is_dir() {
        memory::plan_memory(&source_memory, &destination_memory, &memory_base)
    } else {
        Vec::new()
    };

    let plan = TransferPlan {
        memory: memory_plan
            .into_iter()
            .map(|file| {
                let conflict = file.action == MemoryAction::Conflict;
                TransferMemoryFile {
                    source_text: conflict
                        .then(|| memory::read_lossy(&source_memory.join(&file.rel))),
                    destination_text: conflict
                        .then(|| memory::read_lossy(&destination_memory.join(&file.rel))),
                    path: file.rel,
                    action: file.action,
                    newer: file.newer,
                }
            })
            .collect(),
        session_id: id.to_string(),
        title: source_records
            .iter()
            .find_map(|record| record.title.clone())
            .or_else(|| info.title()),
        cwd: info.cwd.clone(),
        source_label: source.label.clone(),
        destination_label: destination.label.clone(),
        items: items
            .iter()
            .map(|item| PlanItem {
                path: item.rel.display().to_string(),
                action: item.action,
            })
            .collect(),
        destination_newer,
        desktop: desktop_action,
        desktop_reason,
        blockers,
        apps_to_quit,
        notes,
        archive_bytes: size_of(&source_transcript)
            + source_records
                .iter()
                .map(|record| size_of(&record.path))
                .sum::<u64>(),
        source_bytes: items
            .iter()
            .filter(|item| session_own(item))
            .map(|item| size_of(&item.from))
            .chain(source_records.iter().map(|record| size_of(&record.path)))
            .sum(),
    };
    Ok(Prepared {
        plan,
        source,
        destination,
        info,
        items,
        source_transcript,
        source_project,
        source_records,
        destination_records_dir,
        source_memory,
        destination_memory,
        memory_base,
    })
}

/// How long Claude gets to merge a note.
const MERGE_TIMEOUT: Duration = Duration::from_secs(170);

/// Ask Claude, as the destination profile, to merge the two versions of
/// memory note `path` that the move `request` would otherwise have the user
/// choose between, the way claudemulti does: no tools, no saved session, and
/// in safe mode, so no CLAUDE.md, hooks, plugins or MCP servers, from an
/// empty folder of ai-profiles' own. Returns the merge, for the user to
/// accept or not.
pub fn merge_with_claude(request: &TransferRequest, path: &str) -> AppResult<String> {
    let prepared = prepare(request)?;
    let plan = &prepared.plan;
    let file = plan
        .memory
        .iter()
        .find(|file| file.path == path && file.action == MemoryAction::Conflict)
        .ok_or_else(|| {
            AppError::Validation("That memory file isn't one the move needs merged.".into())
        })?;
    let claude = crate::deps::resolve_cli_binary_path("claude").ok_or_else(|| {
        AppError::NotFound("Claude Code (claude) isn't installed on this Mac.".into())
    })?;
    let (source, destination) = (&plan.source_label, &plan.destination_label);
    let newer = match file.newer {
        Side::Source => source,
        Side::Destination => destination,
    };
    let prompt = memory::claude_merge_prompt(
        newer,
        destination,
        file.destination_text.as_deref().unwrap_or_default(),
        source,
        file.source_text.as_deref().unwrap_or_default(),
    );
    let scratch = crate::paths::app_data_dir()?.join(format!("merge-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&scratch)?;
    let mut command = Command::new(&claude);
    command
        .args([
            "-p",
            "--safe-mode",
            "--no-session-persistence",
            "--tools",
            "",
            "--strict-mcp-config",
        ])
        .current_dir(&scratch)
        .env("PATH", crate::deps::shell_path());
    if prepared.destination.stock {
        command.env_remove("CLAUDE_CONFIG_DIR");
    } else {
        command.env("CLAUDE_CONFIG_DIR", &prepared.destination.config_dir);
    }
    let finished = run_within(&mut command, prompt.into_bytes(), MERGE_TIMEOUT);
    let _ = fs::remove_dir_all(&scratch);
    let finished = finished?.ok_or_else(|| {
        AppError::Validation(format!(
            "Claude took more than {} seconds to merge it.",
            MERGE_TIMEOUT.as_secs()
        ))
    })?;
    finished
        .status
        .success()
        .then(|| memory::clean_claude_merge(&String::from_utf8_lossy(&finished.stdout)))
        .flatten()
        .ok_or_else(|| {
            AppError::Validation(format!(
                "Claude couldn't merge it: {}",
                String::from_utf8_lossy(&finished.stderr)
                    .lines()
                    .next()
                    .unwrap_or("no answer")
            ))
        })
}

/// The first memory note the plan needs decided that `decisions` doesn't
/// settle.
fn first_undecided<'a>(
    plan: &'a TransferPlan,
    decisions: &HashMap<String, Decision>,
) -> Option<&'a TransferMemoryFile> {
    plan.memory
        .iter()
        .find(|file| file.action == MemoryAction::Conflict && !decisions.contains_key(&file.path))
}

/// Where the version of a project's memory both profiles last had in common
/// is kept, as claudemulti keeps it beside its accounts: in ai-profiles' own
/// folder, where no Claude loads it. Named after the source's project, and
/// updated from it on every move.
fn memory_base(project: &str) -> AppResult<PathBuf> {
    Ok(crate::paths::app_data_dir()?
        .join("memory-base")
        .join(project))
}

/// The one transcript of session `id` in `home`, as (project, path).
pub(super) fn single_transcript(home: &Home, id: &str) -> AppResult<Option<(String, PathBuf)>> {
    let mut found: Vec<(String, PathBuf)> = scan::transcripts(&home.config_dir)
        .into_iter()
        .filter(|(_, session, _)| session == id)
        .map(|(project, _, path)| (project, path))
        .collect();
    match found.len() {
        0 => Ok(None),
        1 => Ok(found.pop()),
        _ => Err(AppError::Validation(format!(
            "{} has {} copies of session {id}, in {}. Refusing to guess which one is current.",
            home.label,
            found.len(),
            found
                .iter()
                .map(|(project, _)| project.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// Everything that belongs to session `id`, transcript last.
fn items(
    source: &Home,
    destination: &Home,
    id: &str,
    source_project: &str,
    destination_project: &str,
    info: &TranscriptInfo,
) -> AppResult<Vec<Item>> {
    // (path in the source, path in the destination), both relative.
    let mut pairs: Vec<(PathBuf, PathBuf)> = vec![(
        Path::new("projects").join(source_project).join(id),
        Path::new("projects").join(destination_project).join(id),
    )];
    for dir in ["file-history", "session-env", "tasks"] {
        let rel = Path::new(dir).join(id);
        pairs.push((rel.clone(), rel));
    }
    if let Ok(todos) = fs::read_dir(source.config_dir.join("todos")) {
        let prefix = format!("{id}-");
        let mut names: Vec<String> = todos
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(&prefix) && name.ends_with(".json"))
            .collect();
        names.sort();
        for name in names {
            let rel = Path::new("todos").join(name);
            pairs.push((rel.clone(), rel));
        }
    }
    for slug in &info.slugs {
        let rel = Path::new("plans").join(format!("{slug}.md"));
        pairs.push((rel.clone(), rel));
    }

    let mut items = Vec::new();
    for (from_rel, to_rel) in pairs {
        let from = source.config_dir.join(&from_rel);
        if fs::symlink_metadata(&from).is_err() {
            continue;
        }
        items.push(item(from, destination, to_rel)?);
    }
    let transcript = format!("{id}.jsonl");
    items.push(item(
        source
            .config_dir
            .join("projects")
            .join(source_project)
            .join(&transcript),
        destination,
        Path::new("projects")
            .join(destination_project)
            .join(&transcript),
    )?);
    Ok(items)
}

fn item(from: PathBuf, destination: &Home, rel: PathBuf) -> AppResult<Item> {
    let to = destination.config_dir.join(&rel);
    let action = if fs::symlink_metadata(&to).is_err() {
        ItemAction::Copy
    } else if identical(&from, &to)? {
        ItemAction::Same
    } else {
        ItemAction::Replace
    };
    Ok(Item {
        from,
        to,
        rel,
        action,
    })
}

fn execute(
    prepared: &Prepared,
    decisions: &HashMap<String, Decision>,
    afterwards: Afterwards,
    steps: &Steps,
) -> AppResult<TransferReport> {
    let id = &prepared.plan.session_id;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let backup_root = prepared
        .destination
        .config_dir
        .join(BACKUPS_DIR)
        .join(id)
        .join(&stamp);
    let mut backed_up = false;

    steps.at(Step::Copy);
    for item in &prepared.items {
        match item.action {
            ItemAction::Same => continue,
            ItemAction::Replace => {
                move_into(&item.to, &backup_root.join(&item.rel))?;
                backed_up = true;
            }
            ItemAction::Copy => {}
        }
        copy_into_place(&item.from, &item.to)?;
    }

    let memory = if prepared.source_memory.is_dir() {
        memory::apply_memory(
            &prepared.source_memory,
            &prepared.destination_memory,
            &prepared.memory_base,
            &prepared.destination.config_dir,
            &backup_root,
            decisions,
            (&prepared.source.label, &prepared.destination.label),
        )?
    } else {
        memory::MemoryReport::default()
    };
    backed_up |= memory.backed_up;

    steps.at(Step::Desktop);
    let desktop_record = match (&prepared.plan.desktop, &prepared.destination_records_dir) {
        (DesktopAction::Add, Some(dir)) => {
            let source_mtime = modified(&prepared.source_transcript);
            let last_activity_ms = millis(source_mtime);
            let created_at_ms = prepared
                .info
                .first_timestamp
                .as_deref()
                .and_then(|stamp| chrono::DateTime::parse_from_rfc3339(stamp).ok())
                .map(|time| time.timestamp_millis())
                .unwrap_or(last_activity_ms);
            let source_record = prepared
                .source_records
                .iter()
                .find(|record| !record.archived)
                .or(prepared.source_records.first())
                .map(|record| &record.body);
            let name = prepared.info.name();
            let record = desktop::build_record(
                source_record,
                &NewRecord {
                    cli_session_id: id,
                    cwd: prepared.info.cwd.as_deref().unwrap_or_default(),
                    title: name.as_ref().map(|(name, _)| name.as_str()),
                    title_from_user: name.as_ref().is_some_and(|(_, from_user)| *from_user),
                    created_at_ms,
                    last_activity_ms,
                },
            );
            Some(desktop::write_record(dir, &record)?.display().to_string())
        }
        _ => None,
    };

    steps.at(Step::Afterwards);
    let archived_to = if afterwards == Afterwards::Archive {
        let root = archive_files(
            &prepared.source,
            id,
            &prepared.source_project,
            &prepared.source_records,
            &stamp,
        )?;
        Some(root.display().to_string())
    } else {
        None
    };
    let (freed_bytes, delete_error) = if afterwards == Afterwards::Delete {
        match delete_source(&prepared.items, &prepared.source_records) {
            Ok(freed) => (Some(freed), None),
            Err(err) => (
                None,
                Some(format!(
                    "The copy in {} was kept: {err}.",
                    prepared.source.label
                )),
            ),
        }
    } else {
        (None, None)
    };

    Ok(TransferReport {
        freed_bytes,
        delete_error,
        destination_transcript: prepared
            .items
            .last()
            .map(|item| item.to.display().to_string())
            .unwrap_or_default(),
        backup_dir: backed_up.then(|| backup_root.display().to_string()),
        desktop_record,
        archived_to,
        memory: memory.lines,
    })
}

fn modified(path: &Path) -> SystemTime {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn millis(time: SystemTime) -> i64 {
    chrono::DateTime::<chrono::Utc>::from(time).timestamp_millis()
}

/// Whether two files, folders or links hold the same thing.
fn identical(a: &Path, b: &Path) -> AppResult<bool> {
    let (meta_a, meta_b) = (fs::symlink_metadata(a)?, fs::symlink_metadata(b)?);
    let (kind_a, kind_b) = (meta_a.file_type(), meta_b.file_type());
    if kind_a.is_symlink() || kind_b.is_symlink() {
        return Ok(kind_a.is_symlink()
            && kind_b.is_symlink()
            && fs::read_link(a)? == fs::read_link(b)?);
    }
    if kind_a.is_file() && kind_b.is_file() {
        return Ok(meta_a.len() == meta_b.len() && fs::read(a)? == fs::read(b)?);
    }
    if kind_a.is_dir() && kind_b.is_dir() {
        let names = |dir: &Path| -> AppResult<Vec<std::ffi::OsString>> {
            let mut names: Vec<_> = fs::read_dir(dir)?
                .map(|entry| entry.map(|entry| entry.file_name()))
                .collect::<Result<_, _>>()?;
            names.sort();
            Ok(names)
        };
        let (names_a, names_b) = (names(a)?, names(b)?);
        if names_a != names_b {
            return Ok(false);
        }
        for name in names_a {
            if !identical(&a.join(&name), &b.join(&name))? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    Ok(false)
}

/// The temporary name `path` is built under before being renamed into place.
fn temp_path(path: &Path) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!(".{name}.ai-profiles-tmp"))
}

/// Copy `from` (file, folder or link) to `to`, which must not exist, through a
/// temporary name beside it.
fn copy_into_place(from: &Path, to: &Path) -> AppResult<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = temp_path(to);
    remove_any(&tmp)?;
    copy_any(from, &tmp)?;
    fs::rename(&tmp, to)?;
    Ok(())
}

fn copy_any(from: &Path, to: &Path) -> AppResult<()> {
    let kind = fs::symlink_metadata(from)?.file_type();
    if kind.is_symlink() {
        std::os::unix::fs::symlink(fs::read_link(from)?, to)?;
    } else if kind.is_dir() {
        fs::create_dir(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_any(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else {
        fs::copy(from, to)?;
    }
    Ok(())
}

fn remove_any(path: &Path) -> AppResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path)?,
        Ok(_) => fs::remove_file(path)?,
        Err(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn home(root: &Path, name: &str) -> Home {
        Home {
            id: name.into(),
            label: name.into(),
            config_dir: root.join(name).join("cli-config"),
            gui_data_dir: root.join(name).join("gui-data"),
            stock: false,
            desktop: true,
        }
    }

    const ID: &str = "11111111-2222-3333-4444-555555555555";

    fn transcript() -> String {
        format!(
            "{}\n{}\n",
            r#"{"type":"user","cwd":"/work","timestamp":"2026-01-01T00:00:00Z"}"#,
            r#"{"type":"assistant","slug":"the-plan"}"#
        )
    }

    fn seed_source(source: &Home) {
        let config = &source.config_dir;
        write(
            &config.join(format!("projects/-work/{ID}.jsonl")),
            &transcript(),
        );
        write(
            &config.join(format!("projects/-work/{ID}/subagents/a.jsonl")),
            "x",
        );
        write(&config.join(format!("file-history/{ID}/f@v1")), "v1");
        write(&config.join(format!("todos/{ID}-agent-{ID}.json")), "[]");
        write(&config.join("todos/other-agent.json"), "[]");
        write(&config.join("plans/the-plan.md"), "# plan");
        write(&config.join("plans/unrelated.md"), "# no");
    }

    fn rels(items: &[Item]) -> Vec<(String, ItemAction)> {
        items
            .iter()
            .map(|item| (item.rel.display().to_string(), item.action))
            .collect()
    }

    #[test]
    fn items_cover_the_session_and_put_the_transcript_last() {
        let dir = tempfile::tempdir().unwrap();
        let (source, destination) = (home(dir.path(), "a"), home(dir.path(), "b"));
        seed_source(&source);
        let info =
            scan::read_transcript(&source.config_dir.join(format!("projects/-work/{ID}.jsonl")))
                .unwrap();
        let found = items(&source, &destination, ID, "-work", "-work", &info).unwrap();
        assert_eq!(
            rels(&found),
            vec![
                (format!("projects/-work/{ID}"), ItemAction::Copy),
                (format!("file-history/{ID}"), ItemAction::Copy),
                (format!("todos/{ID}-agent-{ID}.json"), ItemAction::Copy),
                ("plans/the-plan.md".to_string(), ItemAction::Copy),
                (format!("projects/-work/{ID}.jsonl"), ItemAction::Copy),
            ]
        );
    }

    #[test]
    fn items_compare_against_what_the_destination_has() {
        let dir = tempfile::tempdir().unwrap();
        let (source, destination) = (home(dir.path(), "a"), home(dir.path(), "b"));
        seed_source(&source);
        write(
            &destination
                .config_dir
                .join(format!("projects/-moved/{ID}.jsonl")),
            "older",
        );
        write(&destination.config_dir.join("plans/the-plan.md"), "# plan");
        let info =
            scan::read_transcript(&source.config_dir.join(format!("projects/-work/{ID}.jsonl")))
                .unwrap();
        let found = items(&source, &destination, ID, "-work", "-moved", &info).unwrap();
        let found = rels(&found);
        assert!(found.contains(&("plans/the-plan.md".to_string(), ItemAction::Same)));
        assert_eq!(
            found.last().unwrap(),
            &(format!("projects/-moved/{ID}.jsonl"), ItemAction::Replace)
        );
    }

    /// Steps that say nothing, for moves whose progress no one watches.
    fn quiet() -> Steps<'static> {
        Steps {
            session_id: String::new(),
            named: Vec::new(),
            report: &|_| {},
        }
    }

    #[test]
    fn a_move_says_each_step_as_it_starts() {
        let dir = tempfile::tempdir().unwrap();
        let prepared = prepared(dir.path());
        let said = std::cell::RefCell::new(Vec::new());
        let report = |progress: &TransferProgress| said.borrow_mut().push(progress.clone());
        let mut plan = prepared.plan.clone();
        plan.apps_to_quit = vec![AppToQuit {
            profile_id: "b".into(),
            label: "b".into(),
        }];
        let steps = Steps::of(&plan, true, Afterwards::Delete, &report);
        assert_eq!(
            steps
                .named
                .iter()
                .map(|(_, name)| name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Quitting Claude (b)",
                "Copying it to b",
                "Adding it to b's desktop app",
                "Deleting the copy in a",
            ]
        );

        let steps = Steps::of(&prepared.plan, false, Afterwards::Keep, &report);
        execute(&prepared, &HashMap::new(), Afterwards::Keep, &steps).unwrap();
        let said = said.borrow();
        assert_eq!(
            said.iter()
                .map(|progress| progress.current)
                .collect::<Vec<_>>(),
            vec![0, 1],
            "copying, then the desktop record; keeping the copy is no step"
        );
        assert_eq!(
            said[0].steps,
            vec!["Copying it to b", "Adding it to b's desktop app"]
        );
        assert_eq!(said[0].session_id, ID);
    }

    fn prepared(dir: &Path) -> Prepared {
        let (source, destination) = (home(dir, "a"), home(dir, "b"));
        seed_source(&source);
        let source_transcript = source.config_dir.join(format!("projects/-work/{ID}.jsonl"));
        let info = scan::read_transcript(&source_transcript).unwrap();
        let found = items(&source, &destination, ID, "-work", "-work", &info).unwrap();
        let source_records = desktop::records(&source.gui_data_dir)
            .into_iter()
            .filter(|record| record.cli_session_id == ID)
            .collect();
        let records_dir = destination
            .gui_data_dir
            .join("claude-code-sessions/acct/org");
        Prepared {
            plan: TransferPlan {
                session_id: ID.into(),
                title: Some("Audit".into()),
                cwd: info.cwd.clone(),
                source_label: "a".into(),
                destination_label: "b".into(),
                items: Vec::new(),
                destination_newer: false,
                desktop: DesktopAction::Add,
                desktop_reason: None,
                blockers: Vec::new(),
                apps_to_quit: Vec::new(),
                notes: Vec::new(),
                source_bytes: 0,
                archive_bytes: 0,
                memory: Vec::new(),
            },
            source,
            destination,
            info,
            items: found,
            source_transcript,
            source_project: "-work".into(),
            source_memory: dir.join("a/cli-config/projects/-work/memory"),
            destination_memory: dir.join("b/cli-config/projects/-work/memory"),
            memory_base: dir.join("memory-base/-work"),
            source_records,
            destination_records_dir: Some(records_dir),
        }
    }

    #[test]
    fn execute_copies_the_session_writes_a_record_and_archives_the_source() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path()
                .join("a/gui-data/claude-code-sessions/acct/org/local_old.json"),
            &format!(
                r#"{{"sessionId":"local_old","cliSessionId":"{ID}","title":"Audit","model":"m","remoteMcpServersConfig":[]}}"#
            ),
        );
        let prepared = prepared(dir.path());
        let report = execute(&prepared, &HashMap::new(), Afterwards::Archive, &quiet()).unwrap();

        let destination = &prepared.destination.config_dir;
        assert_eq!(
            fs::read_to_string(destination.join(format!("projects/-work/{ID}.jsonl"))).unwrap(),
            transcript()
        );
        assert!(destination
            .join(format!("projects/-work/{ID}/subagents/a.jsonl"))
            .is_file());
        assert!(destination
            .join(format!("file-history/{ID}/f@v1"))
            .is_file());
        assert!(destination.join("plans/the-plan.md").is_file());
        assert!(!destination.join("plans/unrelated.md").exists());
        assert!(!destination.join("todos/other-agent.json").exists());
        assert!(report.backup_dir.is_none());

        let record: Value =
            serde_json::from_str(&fs::read_to_string(report.desktop_record.unwrap()).unwrap())
                .unwrap();
        assert_eq!(record["cliSessionId"], ID);
        assert_eq!(record["model"], "m");
        assert!(record.get("remoteMcpServersConfig").is_none());

        let archived = PathBuf::from(report.archived_to.unwrap());
        assert!(!prepared.source_transcript.exists());
        assert!(archived
            .join(format!("projects/-work/{ID}.jsonl.gz"))
            .is_file());
        assert!(archived
            .join("desktop-records/acct/org/local_old.json")
            .is_file());
        // The session's folder stays, so paths inside the transcript still resolve.
        assert!(prepared
            .source
            .config_dir
            .join(format!("projects/-work/{ID}"))
            .is_dir());
        // No temporary files are left behind.
        let leftovers: Vec<_> = fs::read_dir(destination.join("projects/-work"))
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with("ai-profiles-tmp")
            })
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn execute_backs_up_what_it_replaces() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path()
                .join(format!("b/cli-config/projects/-work/{ID}.jsonl")),
            "stale copy",
        );
        let prepared = prepared(dir.path());
        let report = execute(&prepared, &HashMap::new(), Afterwards::Keep, &quiet()).unwrap();
        let backup = PathBuf::from(report.backup_dir.unwrap());
        assert_eq!(
            fs::read_to_string(backup.join(format!("projects/-work/{ID}.jsonl"))).unwrap(),
            "stale copy"
        );
        assert!(prepared.source_transcript.exists());
        assert!(report.archived_to.is_none());
    }

    #[test]
    fn execute_deletes_the_source_copy_once_it_arrived_intact() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path()
                .join("a/gui-data/claude-code-sessions/acct/org/local_old.json"),
            &format!(r#"{{"sessionId":"local_old","cliSessionId":"{ID}","title":"Audit"}}"#),
        );
        let prepared = prepared(dir.path());
        let report = execute(&prepared, &HashMap::new(), Afterwards::Delete, &quiet()).unwrap();

        let source = &prepared.source.config_dir;
        assert_eq!(report.delete_error, None);
        assert!(report.freed_bytes.unwrap() > 0);
        assert!(report.archived_to.is_none());
        assert!(!prepared.source_transcript.exists());
        assert!(!source.join(format!("projects/-work/{ID}")).exists());
        assert!(!source.join(format!("file-history/{ID}")).exists());
        assert!(!dir
            .path()
            .join("a/gui-data/claude-code-sessions/acct/org/local_old.json")
            .exists());
        assert!(
            source.join("plans/the-plan.md").is_file(),
            "plans may be shared"
        );
        assert!(
            !source.join(BACKUPS_DIR).exists(),
            "nothing archived either"
        );
        assert!(prepared
            .destination
            .config_dir
            .join(format!("projects/-work/{ID}.jsonl"))
            .is_file());
    }

    #[test]
    fn delete_keeps_everything_when_a_moved_copy_differs() {
        let dir = tempfile::tempdir().unwrap();
        let prepared = prepared(dir.path());
        for item in &prepared.items {
            copy_into_place(&item.from, &item.to).unwrap();
        }
        let moved = prepared
            .destination
            .config_dir
            .join(format!("file-history/{ID}/f@v1"));
        fs::write(&moved, "changed since").unwrap();
        assert!(delete_source(&prepared.items, &prepared.source_records).is_err());
        assert!(prepared.source_transcript.is_file());
        assert!(prepared
            .source
            .config_dir
            .join(format!("file-history/{ID}/f@v1"))
            .is_file());
    }

    #[test]
    fn execute_merges_memory_as_decided_and_remembers_the_common_version() {
        let dir = tempfile::tempdir().unwrap();
        let memory = |side: &str, file: &str| {
            dir.path()
                .join(side)
                .join("cli-config/projects/-work/memory")
                .join(file)
        };
        write(&memory("a", "rules.md"), "a's rule\n");
        write(&memory("a", "new.md"), "new\n");
        write(&memory("b", "rules.md"), "b's rule\n");
        let mut prepared = prepared(dir.path());
        prepared.plan.memory = memory::plan_memory(
            &prepared.source_memory,
            &prepared.destination_memory,
            &prepared.memory_base,
        )
        .into_iter()
        .map(|file| TransferMemoryFile {
            path: file.rel,
            action: file.action,
            newer: file.newer,
            source_text: None,
            destination_text: None,
        })
        .collect();
        // Never moved before: no common version, so both changed it.
        let rules = prepared
            .plan
            .memory
            .iter()
            .find(|file| file.path == "rules.md")
            .unwrap();
        assert_eq!(rules.action, MemoryAction::Conflict);
        assert_eq!(
            first_undecided(&prepared.plan, &HashMap::new()).map(|file| file.path.as_str()),
            Some("rules.md")
        );

        let decisions = HashMap::from([("rules.md".to_string(), Decision::Source)]);
        assert!(first_undecided(&prepared.plan, &decisions).is_none());
        let report = execute(&prepared, &decisions, Afterwards::Keep, &quiet()).unwrap();
        assert_eq!(
            fs::read_to_string(memory("b", "rules.md")).unwrap(),
            "a's rule\n"
        );
        assert_eq!(fs::read_to_string(memory("b", "new.md")).unwrap(), "new\n");
        let backup = PathBuf::from(report.backup_dir.expect("b's note was backed up"));
        assert_eq!(
            fs::read_to_string(backup.join("projects/-work/memory/rules.md")).unwrap(),
            "b's rule\n"
        );
        assert!(!report.memory.is_empty());
        // The common version now: next time, a note only one side changed
        // merges without asking.
        assert_eq!(
            fs::read_to_string(prepared.memory_base.join("rules.md")).unwrap(),
            "a's rule\n"
        );
    }

    #[test]
    fn identical_compares_folders_deeply() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a/x/y"), "1");
        write(&dir.path().join("b/x/y"), "1");
        assert!(identical(&dir.path().join("a"), &dir.path().join("b")).unwrap());
        write(&dir.path().join("b/x/y"), "2");
        assert!(!identical(&dir.path().join("a"), &dir.path().join("b")).unwrap());
    }
}
