//! Moving a session from one profile to another.
//!
//! [`plan`] says what a move would do without touching anything; [`transfer`]
//! plans again and carries it out. Nothing in the destination is overwritten
//! without a copy of it going to `<config>/session-transfer-backups/<id>/<time>/`
//! first, every file lands under a temporary name before being renamed into
//! place, and the transcript is copied last, so a move cut short leaves the
//! destination without a session rather than with half of one.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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
    /// Go ahead even though the destination's copy is newer than the source's.
    #[serde(default)]
    pub replace_newer: bool,
    /// Quit the apps the plan lists in `apps_to_quit` first.
    #[serde(default)]
    pub quit_apps: bool,
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
    /// Memory files the destination did not have, now copied.
    pub memory_copied: Vec<String>,
    /// Memory files both sides changed differently. The destination's are kept.
    pub memory_conflicts: Vec<String>,
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
    destination_project: String,
    source_records: Vec<DesktopRecord>,
    destination_records_dir: Option<PathBuf>,
}

/// What moving the session would do, without doing any of it.
pub fn plan(request: &TransferRequest) -> AppResult<TransferPlan> {
    Ok(prepare(request)?.plan)
}

/// Move the session. Refuses whatever [`plan`] lists as a blocker, and a
/// newer destination copy unless `replace_newer` is set. Apps the plan lists
/// to quit are quit first when `quit_apps` is set, and refused otherwise.
pub fn transfer(request: &TransferRequest) -> AppResult<TransferReport> {
    let mut prepared = prepare(request)?;
    if prepared.plan.blockers.is_empty()
        && !prepared.plan.apps_to_quit.is_empty()
        && request.quit_apps
    {
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
    execute(&prepared, request.archive_source)
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
    if request.archive_source
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

    let plan = TransferPlan {
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
    };
    Ok(Prepared {
        plan,
        source,
        destination,
        info,
        items,
        source_transcript,
        source_project,
        destination_project,
        source_records,
        destination_records_dir,
    })
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

fn execute(prepared: &Prepared, archive_source: bool) -> AppResult<TransferReport> {
    let id = &prepared.plan.session_id;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let backup_root = prepared
        .destination
        .config_dir
        .join(BACKUPS_DIR)
        .join(id)
        .join(&stamp);
    let mut backed_up = false;

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

    let memory = merge_memory(
        &prepared
            .source
            .config_dir
            .join("projects")
            .join(&prepared.source_project)
            .join("memory"),
        &prepared
            .destination
            .config_dir
            .join("projects")
            .join(&prepared.destination_project)
            .join("memory"),
        &backup_root
            .join("projects")
            .join(&prepared.destination_project)
            .join("memory"),
    )?;
    backed_up |= memory.backed_up;

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
            let title = prepared.plan.title.clone();
            let record = desktop::build_record(
                source_record,
                &NewRecord {
                    cli_session_id: id,
                    cwd: prepared.info.cwd.as_deref().unwrap_or_default(),
                    title: title.as_deref(),
                    title_from_user: prepared.info.custom_title.is_some(),
                    created_at_ms,
                    last_activity_ms,
                },
            );
            Some(desktop::write_record(dir, &record)?.display().to_string())
        }
        _ => None,
    };

    let archived_to = if archive_source {
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

    Ok(TransferReport {
        destination_transcript: prepared
            .items
            .last()
            .map(|item| item.to.display().to_string())
            .unwrap_or_default(),
        backup_dir: backed_up.then(|| backup_root.display().to_string()),
        desktop_record,
        archived_to,
        memory_copied: memory.copied,
        memory_conflicts: memory.conflicts,
    })
}

#[derive(Default)]
struct MemoryMerge {
    copied: Vec<String>,
    conflicts: Vec<String>,
    backed_up: bool,
}

/// Bring the source project's memory into the destination's without losing
/// anything the destination has: missing files are copied, identical ones
/// left, and ones that differ kept as the destination has them and reported.
/// `MEMORY.md`, the index, gains the source's lines it lacks that point at
/// files the destination now has.
fn merge_memory(from: &Path, to: &Path, backup: &Path) -> AppResult<MemoryMerge> {
    let mut merge = MemoryMerge::default();
    let Ok(entries) = fs::read_dir(from) else {
        return Ok(merge);
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "MEMORY.md")
        .collect();
    names.sort();
    for name in names {
        let source = from.join(&name);
        let target = to.join(&name);
        if !target.exists() {
            copy_into_place(&source, &target)?;
            merge.copied.push(name);
        } else if !identical(&source, &target)? {
            merge.conflicts.push(name);
        }
    }

    let source_index = from.join("MEMORY.md");
    let target_index = to.join("MEMORY.md");
    if source_index.is_file() {
        if !target_index.exists() {
            copy_into_place(&source_index, &target_index)?;
            merge.copied.push("MEMORY.md".to_string());
        } else {
            let theirs = fs::read_to_string(&source_index)?;
            let ours = fs::read_to_string(&target_index)?;
            let merged = merge_index(&ours, &theirs, |file| to.join(file).exists());
            if merged != ours {
                fs::create_dir_all(backup)?;
                fs::copy(&target_index, backup.join("MEMORY.md"))?;
                merge.backed_up = true;
                write_into_place(&target_index, merged.as_bytes())?;
            }
        }
    }
    Ok(merge)
}

/// `ours` plus every line of `theirs` it lacks, except index lines pointing at
/// a file `exists` says is missing.
fn merge_index(ours: &str, theirs: &str, exists: impl Fn(&str) -> bool) -> String {
    let have: std::collections::HashSet<&str> = ours.lines().map(str::trim_end).collect();
    let mut merged = ours.to_string();
    for line in theirs.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() || have.contains(line) {
            continue;
        }
        if linked_file(line).is_some_and(|file| !exists(file)) {
            continue;
        }
        if !merged.is_empty() && !merged.ends_with('\n') {
            merged.push('\n');
        }
        merged.push_str(line);
        merged.push('\n');
    }
    merged
}

/// The file a `- [Title](file.md) — hook` index line links to.
fn linked_file(line: &str) -> Option<&str> {
    let start = line.find("](")? + 2;
    let end = start + line[start..].find(')')?;
    let file = &line[start..end];
    (is_safe_name(file) && file.ends_with(".md")).then_some(file)
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

fn write_into_place(to: &Path, bytes: &[u8]) -> AppResult<()> {
    let tmp = temp_path(to);
    fs::write(&tmp, bytes)?;
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
            },
            source,
            destination,
            info,
            items: found,
            source_transcript,
            source_project: "-work".into(),
            destination_project: "-work".into(),
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
        let report = execute(&prepared, true).unwrap();

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
            .join(format!("projects/-work/{ID}.jsonl"))
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
        let report = execute(&prepared, false).unwrap();
        let backup = PathBuf::from(report.backup_dir.unwrap());
        assert_eq!(
            fs::read_to_string(backup.join(format!("projects/-work/{ID}.jsonl"))).unwrap(),
            "stale copy"
        );
        assert!(prepared.source_transcript.exists());
        assert!(report.archived_to.is_none());
    }

    #[test]
    fn memory_merge_keeps_the_destinations_changes() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to, backup) = (
            dir.path().join("from"),
            dir.path().join("to"),
            dir.path().join("backup"),
        );
        write(&from.join("new.md"), "new");
        write(&from.join("same.md"), "same");
        write(&from.join("changed.md"), "source");
        write(
            &from.join("MEMORY.md"),
            "- [Same](same.md) — s\n- [New](new.md) — n\n- [Gone](gone.md) — g\n",
        );
        write(&to.join("same.md"), "same");
        write(&to.join("changed.md"), "destination");
        write(&to.join("MEMORY.md"), "- [Same](same.md) — s\n");

        let merge = merge_memory(&from, &to, &backup).unwrap();
        assert_eq!(merge.copied, vec!["new.md".to_string()]);
        assert_eq!(merge.conflicts, vec!["changed.md".to_string()]);
        assert_eq!(
            fs::read_to_string(to.join("changed.md")).unwrap(),
            "destination"
        );
        assert_eq!(
            fs::read_to_string(to.join("MEMORY.md")).unwrap(),
            "- [Same](same.md) — s\n- [New](new.md) — n\n"
        );
        assert_eq!(
            fs::read_to_string(backup.join("MEMORY.md")).unwrap(),
            "- [Same](same.md) — s\n"
        );
    }

    #[test]
    fn linked_file_reads_index_lines() {
        assert_eq!(linked_file("- [A](a.md) — hook"), Some("a.md"));
        assert_eq!(linked_file("- [A](../a.md)"), None);
        assert_eq!(linked_file("plain text"), None);
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
