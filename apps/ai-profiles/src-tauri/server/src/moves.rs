//! Moving a session to another account on the host, archiving and restoring
//! one, and noticing Claude already running where a session is about to
//! start: claudemulti's session management, over the API.
//!
//! claudemulti asks its questions in the terminal as it goes. Here they are
//! asked up front instead: [`plan`] reports everything a move would do and
//! everything it needs decided, and [`transfer`] plans again, refuses unless
//! each of those was answered, then does it.

use std::collections::HashMap;
use std::fs;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use ai_profiles_core::api::{
    ArchivedSession, RunningMatch, TmuxWindow, TransferItem, TransferMemoryFile, TransferPlan,
};
use ai_profiles_core::memory::{self, Decision, MemoryAction, MemoryReport};
use ai_profiles_core::session_move::{self, Item, ItemAction, MoveError};
use ai_profiles_core::transcript::read_transcript;

use crate::accounts::{self, AccountDir};
use crate::config::Config;
use crate::error::ApiError;
use crate::procs::ProcessTable;
use crate::sessions;
use ai_profiles_core::child::run_within;

/// Every running Claude, in any account, that has session `id` open or works
/// in `cwd` (the same folder, not one inside it). `except` is a session
/// that doesn't count, being the one about to start.
pub fn running_beside(
    config: &Config,
    processes: &dyn ProcessTable,
    cwd: Option<&str>,
    id: Option<&str>,
) -> Vec<RunningMatch> {
    let folder = cwd.map(canonical);
    let mut found = Vec::new();
    for account in accounts::discover(config) {
        let mut running: Vec<_> = sessions::running(&account, processes).into_iter().collect();
        running.sort_by_key(|(_, entry)| entry.pid);
        for (session, entry) in running {
            let exact = id == Some(session.as_str());
            let same_folder = folder.is_some() && entry.cwd.as_deref().map(canonical) == folder;
            if !exact && !same_folder {
                continue;
            }
            found.push(RunningMatch {
                account: account.name.clone(),
                pid: entry.pid,
                window: entry.tmux_location().map(|location| TmuxWindow {
                    session: location.session,
                    window_id: location.window_id,
                    pane_id: location.pane_id,
                }),
                cwd: entry.cwd.clone(),
                session_id: Some(session),
                exact,
            });
        }
    }
    found
}

fn canonical(path: &str) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path))
}

/// One line per running Claude, for a refusal a person reads.
pub fn describe_running(running: &[RunningMatch]) -> String {
    running
        .iter()
        .map(|found| {
            format!(
                "{} under {} (process {}){}",
                if found.exact {
                    "this session"
                } else {
                    "another session"
                },
                found.account,
                found.pid,
                found
                    .cwd
                    .as_deref()
                    .map(|cwd| format!(" in {cwd}"))
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// A move worked out, ready to carry out.
pub struct Prepared {
    pub plan: TransferPlan,
    pub source: AccountDir,
    pub destination: AccountDir,
    items: Vec<Item>,
    source_transcript: PathBuf,
    source_memory: PathBuf,
    destination_memory: PathBuf,
    memory_base: PathBuf,
}

/// The one transcript of session `id` in `account`.
pub fn transcript_of(account: &AccountDir, id: &str) -> Result<PathBuf, ApiError> {
    let mut found = session_move::transcripts_of(&account.dir, id);
    match found.len() {
        0 => Err(ApiError::not_found(format!(
            "{} has no session with that id.",
            account.name
        ))),
        1 => Ok(found.remove(0)),
        count => Err(ApiError::conflict(
            "ambiguous_session",
            format!(
                "{} has {count} copies of this session. Refusing to guess which one is current.",
                account.name
            ),
        )),
    }
}

/// What moving session `id` from `source` to `destination` would do.
pub fn plan(
    config: &Config,
    processes: &dyn ProcessTable,
    source: AccountDir,
    id: &str,
    destination: AccountDir,
) -> Result<Prepared, ApiError> {
    // By where the folders really are: an account can be a link to another,
    // and moving a session onto itself, then deleting "the source", would
    // delete the only copy.
    let real = |dir: &Path| fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if real(&source.dir) == real(&destination.dir) {
        return Err(ApiError::invalid(format!(
            "The session is already in {}.",
            destination.name
        )));
    }
    let source_transcript = transcript_of(&source, id)?;
    let info = read_transcript(&source_transcript).map_err(ApiError::internal)?;
    let destination_transcript =
        session_move::destination_transcript(&source.dir, &source_transcript, &destination.dir, id)
            .map_err(|err| match err {
                MoveError::AmbiguousDestination(_) => {
                    ApiError::conflict("ambiguous_destination", err.to_string())
                }
                MoveError::Io(message) => ApiError::internal(message),
            })?;
    let items = session_move::plan_items(
        &source.dir,
        &destination.dir,
        id,
        &source_transcript,
        &destination_transcript,
        &info.slugs,
    )
    .map_err(ApiError::internal)?;
    let destination_newer =
        session_move::destination_newer(&source_transcript, &destination_transcript);

    let source_project = source_transcript.parent().unwrap_or(&source.dir);
    let project_key = source_project
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let source_memory = source_project.join("memory");
    let destination_memory = destination_transcript
        .parent()
        .unwrap_or(&destination.dir)
        .join("memory");
    let memory_base = memory::base_dir(&config.accounts_base, &project_key);
    let memory_plan = if source_memory.is_dir() {
        memory::plan_memory(&source_memory, &destination_memory, &memory_base)
    } else {
        Vec::new()
    };

    let plan = TransferPlan {
        session_id: id.to_owned(),
        source: source.name.clone(),
        destination: destination.name.clone(),
        title: info.title(),
        cwd: info.cwd.clone(),
        items: items
            .iter()
            .map(|item| TransferItem {
                path: item.rel.display().to_string(),
                action: item.action,
            })
            .collect(),
        destination_newer,
        source_bytes: session_move::source_bytes(&items),
        archive_bytes: session_move::size_of(&source_transcript),
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
        // Only the session itself: other sessions in its folder are ordinary.
        running: running_beside(config, processes, None, Some(id)),
    };
    Ok(Prepared {
        plan,
        source,
        destination,
        items,
        source_transcript,
        source_memory,
        destination_memory,
        memory_base,
    })
}

/// What [`transfer`] did, before any resume.
pub struct Moved {
    pub changed: bool,
    pub backup_dir: Option<String>,
    pub memory: Vec<String>,
    pub archived_to: Option<String>,
    pub freed_bytes: Option<u64>,
    pub delete_error: Option<String>,
}

/// What becomes of the source's copy once a session has moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Afterwards {
    Keep,
    /// Its transcript goes to its backups, and can be restored.
    Archive,
    /// Deleted, once the moved copy is checked to be identical.
    Delete,
}

/// Carry out `prepared`, with the plan's questions answered, or refuse with
/// the first that wasn't. `stamp` names the backup and archive folders.
pub fn transfer(
    prepared: &Prepared,
    replace_newer: bool,
    confirm_running: bool,
    decisions: &HashMap<String, Decision>,
    afterwards: Afterwards,
    stamp: &str,
) -> Result<Moved, ApiError> {
    let plan = &prepared.plan;
    if !plan.running.is_empty() && !confirm_running {
        return Err(ApiError::conflict(
            "session_running",
            format!(
                "Claude is already running: {}.",
                describe_running(&plan.running)
            ),
        ));
    }
    // Claude keeps writing to a running session's transcript: archived or
    // deleted from under it, the copy that moved would be missing the rest.
    if !plan.running.is_empty() && afterwards != Afterwards::Keep {
        return Err(ApiError::conflict(
            "session_running",
            format!(
                "Claude is still running it ({}). Stop it first to take it out of {}.",
                describe_running(&plan.running),
                plan.source
            ),
        ));
    }
    if plan.destination_newer && !replace_newer {
        return Err(ApiError::conflict(
            "destination_newer",
            format!(
                "{} has a newer copy of this session. Moving it would roll that copy back.",
                plan.destination
            ),
        ));
    }
    if let Some(undecided) = plan
        .memory
        .iter()
        .find(|file| file.action == MemoryAction::Conflict && !decisions.contains_key(&file.path))
    {
        return Err(ApiError::conflict(
            "memory_decision_needed",
            format!(
                "memory/{} differs, and both sides changed it: say which to keep.",
                undecided.path
            ),
        ));
    }

    let id = &plan.session_id;
    let backup_root = session_move::backup_root(&prepared.destination.dir, id, stamp);
    let mut backed_up =
        session_move::apply_items(&prepared.items, &backup_root).map_err(ApiError::internal)?;
    let report = if prepared.source_memory.is_dir() {
        memory::apply_memory(
            &prepared.source_memory,
            &prepared.destination_memory,
            &prepared.memory_base,
            &prepared.destination.dir,
            &backup_root,
            decisions,
            (&prepared.source.name, &prepared.destination.name),
        )
        .map_err(ApiError::internal)?
    } else {
        MemoryReport::default()
    };
    backed_up |= report.backed_up;
    let archived_to = if afterwards == Afterwards::Archive {
        Some(
            session_move::archive_transcript(
                &prepared.source.dir,
                &prepared.source_transcript,
                stamp,
            )
            .map_err(ApiError::internal)?
            .display()
            .to_string(),
        )
    } else {
        None
    };
    let (freed_bytes, delete_error) = if afterwards == Afterwards::Delete {
        match session_move::delete_source(&prepared.items) {
            Ok(freed) => (Some(freed), None),
            Err(err) => (
                None,
                Some(format!(
                    "The copy in {} was kept: {err}.",
                    prepared.source.name
                )),
            ),
        }
    } else {
        (None, None)
    };
    Ok(Moved {
        freed_bytes,
        delete_error,
        changed: prepared
            .items
            .iter()
            .any(|item| item.action != ItemAction::Same),
        backup_dir: backed_up.then(|| backup_root.display().to_string()),
        memory: report.lines,
        archived_to,
    })
}

/// Ask Claude, under `destination`, to merge the two versions of memory note
/// `path` that a move from `source` would otherwise make the user choose
/// between, the way claudemulti does: no tools, no saved session, and in
/// safe mode, so no CLAUDE.md, hooks, plugins or MCP servers, from an empty
/// folder of the server's own (not `/tmp`, where anyone could leave a
/// CLAUDE.md for it to find). Returns the merged note.
pub fn claude_merge(
    config: &Config,
    state_dir: &Path,
    prepared: &Prepared,
    path: &str,
) -> Result<String, ApiError> {
    let file = prepared
        .plan
        .memory
        .iter()
        .find(|file| file.path == path && file.action == MemoryAction::Conflict)
        .ok_or_else(|| ApiError::invalid("That memory file isn't one the move needs merged."))?;
    let claude = crate::hostinfo::claude_path(config).ok_or_else(|| {
        ApiError::unavailable(
            "claude_not_found",
            "claude isn't installed where the server looks. Set claude_path in its config.toml.",
        )
    })?;
    let (source, destination) = (&prepared.source.name, &prepared.destination.name);
    let newer = match file.newer {
        memory::Side::Source => source,
        memory::Side::Destination => destination,
    };
    let prompt = memory::claude_merge_prompt(
        newer,
        destination,
        file.destination_text.as_deref().unwrap_or_default(),
        source,
        file.source_text.as_deref().unwrap_or_default(),
    );
    let scratch = scratch_dir(state_dir)?;
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
        .env("CLAUDE_CONFIG_DIR", &prepared.destination.dir);
    let finished = run_within(&mut command, prompt.into_bytes(), MERGE_TIMEOUT);
    let _ = fs::remove_dir_all(&scratch);
    let finished = finished.map_err(ApiError::internal)?.ok_or_else(|| {
        ApiError::conflict(
            "merge_failed",
            format!(
                "Claude took more than {} seconds to merge it.",
                MERGE_TIMEOUT.as_secs()
            ),
        )
    })?;
    finished
        .status
        .success()
        .then(|| memory::clean_claude_merge(&String::from_utf8_lossy(&finished.stdout)))
        .flatten()
        .ok_or_else(|| {
            ApiError::conflict(
                "merge_failed",
                format!(
                    "Claude couldn't merge it: {}",
                    String::from_utf8_lossy(&finished.stderr)
                        .lines()
                        .next()
                        .unwrap_or("no answer")
                ),
            )
        })
}

/// How long Claude gets to merge a note: less than the Mac waits.
const MERGE_TIMEOUT: Duration = Duration::from_secs(170);

/// An empty folder for `claude -p` to run in, inside the server's state
/// folder, which only its user can enter.
fn scratch_dir(state_dir: &Path) -> Result<PathBuf, ApiError> {
    let dir = state_dir.join(format!("merge-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).map_err(ApiError::internal)?;
    Ok(dir)
}

/// Archive session `id` of `account`: its transcript moves into
/// `session-transfer-backups`. Refused while Claude has the session open, in
/// any account.
pub fn archive(
    config: &Config,
    processes: &dyn ProcessTable,
    account: &AccountDir,
    id: &str,
    stamp: &str,
) -> Result<PathBuf, ApiError> {
    let transcript = transcript_of(account, id)?;
    if let Some(open) = running_beside(config, processes, None, Some(id)).first() {
        return Err(ApiError::conflict(
            "session_running",
            format!(
                "It's running (process {}, under {}). Stop it first.",
                open.pid, open.account
            ),
        ));
    }
    session_move::archive_transcript(&account.dir, &transcript, stamp).map_err(ApiError::internal)
}

/// The archived sessions of `account`, newest first. Each is read through
/// `cache`: an archive is big, and doesn't change.
pub fn archived(account: &AccountDir, cache: &sessions::TranscriptCache) -> Vec<ArchivedSession> {
    session_move::list_archived(&account.dir)
        .into_iter()
        .map(|archived| {
            let info = cache.info(&archived.path).unwrap_or_default();
            ArchivedSession {
                title: info.title().or(info.last_prompt.clone()),
                cwd: info.cwd,
                size_bytes: session_move::size_of(&archived.path),
                id: archived.id,
                archive: archived.archive,
                stamp: archived.stamp,
            }
        })
        .collect()
}

/// Compress the archives made before archives were, in every account, and
/// say what that freed. Run once, as the server starts.
pub fn compress_old_archives(config: &Config) -> Vec<String> {
    let mut said = Vec::new();
    for account in accounts::discover(config) {
        for (path, outcome) in session_move::compress_archived(&account.dir) {
            said.push(match outcome {
                Ok((before, after)) => format!(
                    "compressed an archive in {}: {} MB to {} MB",
                    account.name,
                    before >> 20,
                    after >> 20
                ),
                Err(err) => format!("could not compress {}: {err}", path.display()),
            });
        }
    }
    said
}

/// Put archive `archive` of session `id` back in `account`.
pub fn restore(account: &AccountDir, id: &str, archive: &str) -> Result<PathBuf, ApiError> {
    let found = session_move::list_archived(&account.dir)
        .into_iter()
        .find(|archived| archived.id == id && archived.archive == archive)
        .ok_or_else(|| ApiError::not_found("There is no such archive of that session."))?;
    session_move::restore(&account.dir, &found).map_err(|err| {
        if err.kind() == std::io::ErrorKind::AlreadyExists {
            ApiError::conflict("session_exists", err.to_string())
        } else {
            ApiError::internal(err)
        }
    })
}

/// Delete archive `archive` of session `id` in `account` for good: the folder
/// the archive made in its backups, and the session's backups folder once
/// nothing else is left in it. Returns what it freed.
pub fn delete_archive(account: &AccountDir, id: &str, archive: &str) -> Result<u64, ApiError> {
    let found = session_move::list_archived(&account.dir)
        .into_iter()
        .find(|archived| archived.id == id && archived.archive == archive)
        .ok_or_else(|| ApiError::not_found("There is no such archive of that session."))?;
    let session_backups = account.dir.join(session_move::BACKUPS_DIR).join(&found.id);
    let folder = session_backups.join(&found.archive);
    let freed = session_move::size_of(&folder);
    std::fs::remove_dir_all(&folder).map_err(ApiError::internal)?;
    // Only goes when empty: other archives and backups stay.
    let _ = std::fs::remove_dir(&session_backups);
    Ok(freed)
}

/// Stops a path from carrying more than a name: memory paths come from the
/// client.
pub fn is_memory_path(path: &str) -> bool {
    !path.is_empty()
        && !Path::new(path).is_absolute()
        && Path::new(path)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}
