//! Moving a Codex session to another profile of the app.
//!
//! A move goes through both homes' app-servers and touches no Codex state db
//! directly: the thread's rollout is copied, as it is, into the destination's
//! `archived_sessions/` under its own name; `thread/unarchive` there makes
//! the destination take it in, putting it back among its sessions; and
//! `thread/archive` at the source archives it there, so Restore undoes the
//! move. A copy the destination wouldn't take is set aside, never removed.

use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};

use serde_json::json;

use super::actions::{
    apply_with, check_thread, held_lock, is_archived, ps_output, read_thread, resolve_error,
    Target, CODEX_HAS_IT_OPEN,
};
use crate::app_kind::AppKind;
use crate::codex_rpc::{CodexRpc, CodexRpcError, CodexTransport};
use crate::error::{AppError, AppResult};
use crate::sessions::actions::{blocking, AppToQuit, SessionAction};
use crate::sessions::fs_ops::{move_new, occupied, place_new};
use crate::sessions::instance::{desktop_pid, running_again};
use crate::sessions::move_plan::{DesktopAction, ItemAction, MovePlan, MoveReport, PlannedItem};
use crate::sessions::Home;

/// Where, in a destination's `archived_sessions/`, a copy it wouldn't take
/// is set aside.
const FAILED_DIR: &str = ".ai-profiles-failed";

/// What a set-aside copy's name ends with. App-server finds a thread by a
/// `.jsonl` file named after it anywhere under `archived_sessions/`, so a copy
/// set aside under its own name would still count as the destination's.
const FAILED_SUFFIX: &str = "failed";

/// Why a thread without a rollout file can't move.
const NO_ROLLOUT: &str = "Codex has no file of it to move";

/// A planned move, with the app-servers of both homes it goes through.
pub struct Prepared<S = CodexRpc, D = CodexRpc> {
    /// What the move does, as the user is shown it.
    pub plan: MovePlan,
    /// The source's app-server.
    from: S,
    /// The destination's app-server.
    to: D,
    /// Where the session is.
    source: Home,
    /// Where it goes.
    destination: Home,
    /// The thread's id.
    session_id: String,
    /// The thread's rollout file at the source, if it has one.
    rollout: Option<PathBuf>,
}

/// What moving Codex session `session_id` of `source` to `destination` would
/// do, through app-servers started on both homes. They stay up in what is
/// returned, for [`execute`] to use.
pub async fn plan(source: &Home, destination: &Home, session_id: &str) -> AppResult<Prepared> {
    refuse_other(source, destination)?;
    let from = CodexRpc::start(&source.config_dir)
        .await
        .map_err(|error| start_error(&error))?;
    let to = CodexRpc::start(&destination.config_dir)
        .await
        .map_err(|error| start_error(&error))?;
    let ps_output = ps_output().await?;
    plan_with(from, to, source, destination, session_id, &ps_output).await
}

/// [`plan`], through `from` at the source and `to` at the destination, given
/// the output of `ps -ax -o pid=,command=`.
///
/// The move is blocked while archiving the session at the source is (it is
/// open in Codex, or archived already), and as [`look_at_files`] says, which
/// runs on a blocking thread. Each home's desktop app that runs has to quit
/// first: it keeps its own thread catalog.
pub(super) async fn plan_with<S: CodexTransport, D: CodexTransport>(
    mut from: S,
    mut to: D,
    source: &Home,
    destination: &Home,
    session_id: &str,
    ps_output: &str,
) -> AppResult<Prepared<S, D>> {
    refuse_other(source, destination)?;
    let (archive, thread) = check_thread(
        &mut from,
        source,
        session_id,
        SessionAction::Archive,
        ps_output,
    )
    .await?;
    let there = match read_thread(&mut to, session_id).await {
        Ok(there) => Some(there.path),
        Err(error) => match resolve_error(&error, session_id, destination) {
            AppError::NotFound(_) => None,
            other => return Err(other),
        },
    };
    let files = {
        let (source, destination) = (source.clone(), destination.clone());
        let (session_id, path) = (session_id.to_string(), thread.path);
        blocking(move || look_at_files(&source, &destination, &session_id, path, there)).await?
    };
    let mut blockers: Vec<String> = archive.check.blocker.into_iter().collect();
    blockers.extend(files.blockers);
    let mut apps_to_quit: Vec<AppToQuit> = archive.check.app_to_quit.into_iter().collect();
    if desktop_pid(destination, ps_output).is_some() {
        apps_to_quit.push(AppToQuit::of(destination));
    }
    let plan = MovePlan {
        summary: format!(
            "Moves 1 file from {} to {}",
            source.label, destination.label
        ),
        items: files.items,
        destination_newer: false,
        desktop: DesktopAction::NoDesktop,
        blockers,
        apps_to_quit,
        notes: Vec::new(),
    };
    Ok(Prepared {
        plan,
        from,
        to,
        source: source.clone(),
        destination: destination.clone(),
        session_id: session_id.to_string(),
        rollout: files.rollout,
    })
}

/// What the files of a move from `source` to `destination` of thread
/// `session_id` say about it.
struct Files {
    /// The thread's rollout file at the source, if it has one.
    rollout: Option<PathBuf>,
    /// What the files stand in the way of the move with.
    blockers: Vec<String>,
    /// What the move copies.
    items: Vec<PlannedItem>,
}

/// What the files of a move of thread `session_id` from `source`, where its
/// rollout is at `path`, to `destination` say about it; `there` is where the
/// destination has the thread's rollout, if it has the thread at all.
///
/// The move is blocked while the destination has the thread already, while
/// the thread has no rollout to copy and while a file by the rollout's name
/// sits in the destination's `archived_sessions/`.
fn look_at_files(
    source: &Home,
    destination: &Home,
    session_id: &str,
    path: Option<PathBuf>,
    there: Option<Option<PathBuf>>,
) -> AppResult<Files> {
    let rollout =
        path.filter(|path| fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file()));
    // An archived source is blocked anyway; its file lives elsewhere.
    if let Some(path) = rollout.as_deref() {
        if !is_archived(&source.config_dir, Some(path)) {
            confine(path, source, session_id)?;
        }
    }
    let mut blockers = Vec::new();
    match there {
        Some(there) if is_archived(&destination.config_dir, there.as_deref()) => {
            blockers.push(format!(
                "{} has it archived — restore it there instead",
                destination.label
            ));
        }
        Some(_) => blockers.push(format!("{} has this session already", destination.label)),
        None => {}
    }
    let items = match &rollout {
        None => {
            blockers.push(NO_ROLLOUT.to_string());
            Vec::new()
        }
        Some(path) => {
            let name = file_name(path)?;
            if occupied(&copy_path(destination, &name)) {
                blockers.push(format!(
                    "{} already has a file named {name} in archived_sessions",
                    destination.label
                ));
            }
            vec![PlannedItem {
                path: planned_path(path, source, &name),
                action: ItemAction::Copy,
            }]
        }
    };
    Ok(Files {
        rollout,
        blockers,
        items,
    })
}

/// Carry out `prepared`, a move [`plan`] let through.
pub async fn execute(prepared: Prepared) -> AppResult<MoveReport> {
    execute_with(prepared, ps_output).await
}

/// [`execute`], reading the process list with `processes` each time it looks
/// for a desktop app.
///
/// Right before anything is written, the rollout must still be one of the
/// source's session files, the session still closed at the source (its writer
/// lock, by `lsof`) and neither home's desktop app running. Then the rollout
/// is copied into the destination's `archived_sessions/`, refusing to replace
/// anything there, and unarchived at the destination.
///
/// A failed unarchive doesn't say whether the destination took the session
/// (a timeout, or app-server exiting, may come after it did), so the
/// destination is asked again. Taken, the move goes on. Not taken, the copy,
/// if it is still where it was put, is set aside in
/// `archived_sessions/.ai-profiles-failed/`, and the source left as it was.
/// When that can't be told, nothing more is done and the error says so.
///
/// Last, the session is archived at the source, re-checking it there as
/// archiving always does; if that fails, the destination has it already, so
/// the error says both, and how to finish.
pub(super) async fn execute_with<S, D, P, F>(
    prepared: Prepared<S, D>,
    mut processes: P,
) -> AppResult<MoveReport>
where
    S: CodexTransport,
    D: CodexTransport,
    P: FnMut() -> F,
    F: Future<Output = AppResult<String>>,
{
    let Prepared {
        mut from,
        mut to,
        source,
        destination,
        session_id,
        rollout,
        ..
    } = prepared;
    let rollout = rollout.ok_or_else(|| AppError::Validation(NO_ROLLOUT.to_string()))?;
    {
        let (rollout, source, session_id) = (rollout.clone(), source.clone(), session_id.clone());
        blocking(move || confine(&rollout, &source, &session_id)).await?;
    }
    if held_lock(&source.config_dir, &session_id).await? {
        return Err(AppError::Validation(CODEX_HAS_IT_OPEN.to_string()));
    }
    let ps_output = processes().await?;
    for home in [&source, &destination] {
        if desktop_pid(home, &ps_output).is_some() {
            return Err(running_again(home));
        }
    }
    let copy = copy_path(&destination, &file_name(&rollout)?);
    let target = copy.clone();
    blocking(move || place_new(&rollout, &target)).await?;
    if let Err(error) = to
        .request("thread/unarchive", json!({ "threadId": session_id }))
        .await
    {
        let taken = taken(&mut to, &destination, &session_id).await;
        if taken != Some(true) {
            let (source, destination) = (source.clone(), destination.clone());
            let refusal = blocking(move || {
                Ok(match taken {
                    Some(_) => not_taken(&copy, &destination, &error),
                    None => undecided(&copy, &source, &destination, &error),
                })
            });
            return Err(refusal.await.unwrap_or_else(|failed| failed));
        }
    }
    let ps_output = processes().await?;
    let target = Target { id: session_id };
    apply_with(
        &mut from,
        &source,
        &target,
        SessionAction::Archive,
        &ps_output,
    )
    .await
    .map_err(|error| {
        AppError::Validation(format!(
            "Moved to {}, but couldn't archive it in {} ({}). Archive it in {} to finish.",
            destination.label,
            source.label,
            error.message(),
            source.label
        ))
    })?;
    Ok(MoveReport::default())
}

/// Refuse a move from `source` to `destination` unless both are Codex homes,
/// and different ones.
fn refuse_other(source: &Home, destination: &Home) -> AppResult<()> {
    if destination.app != source.app || source.app != AppKind::Codex {
        return Err(AppError::Validation(format!(
            "{} isn't a Codex profile",
            destination.label
        )));
    }
    if destination.id == source.id || destination.config_dir == source.config_dir {
        return Err(AppError::Validation(format!(
            "It's already in {}",
            source.label
        )));
    }
    Ok(())
}

/// The error starting app-server for a move fails with.
fn start_error(error: &CodexRpcError) -> AppError {
    match error {
        CodexRpcError::NotInstalled => AppError::Validation(
            "Codex CLI not found. Install it to move this session.".to_string(),
        ),
        _ => AppError::Validation(format!("Codex: {error}")),
    }
}

/// The name of the rollout at `path`.
fn file_name(path: &Path) -> AppResult<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| AppError::Validation(NO_ROLLOUT.to_string()))
}

/// Where a rollout named `name` is copied to in `destination`.
fn copy_path(destination: &Home, name: &str) -> PathBuf {
    destination.config_dir.join("archived_sessions").join(name)
}

/// How the plan names the rollout at `path`, named `name`: relative to the
/// source's config dir, which is also where the destination puts it back when
/// it takes it in; just its name when it lives elsewhere. App-server names
/// paths resolved, so the config dir is resolved too before comparing.
fn planned_path(path: &Path, source: &Home, name: &str) -> String {
    let config_dir =
        fs::canonicalize(&source.config_dir).unwrap_or_else(|_| source.config_dir.clone());
    path.strip_prefix(&config_dir)
        .or_else(|_| path.strip_prefix(&source.config_dir))
        .map_or_else(
            |_| name.to_string(),
            |relative| relative.display().to_string(),
        )
}

/// Refuse `path` as the rollout of thread `session_id` of `source` unless it
/// is one of the source's session files, named for that thread: a file under
/// `<config dir>/sessions/`, resolved, named `rollout-…-<id>.jsonl` or
/// `….jsonl.zst`. App-server names the path; this keeps a move from copying
/// anything else out of the home.
fn confine(path: &Path, source: &Home, session_id: &str) -> AppResult<()> {
    let sessions = fs::canonicalize(source.config_dir.join("sessions"));
    let inside = fs::canonicalize(path)
        .is_ok_and(|resolved| sessions.is_ok_and(|sessions| resolved.starts_with(sessions)));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let named = name.starts_with("rollout-")
        && [".jsonl", ".jsonl.zst"]
            .iter()
            .any(|extension| name.ends_with(&format!("-{session_id}{extension}")));
    if inside && named {
        return Ok(());
    }
    Err(AppError::Validation(format!(
        "{} isn't one of {}'s session files",
        path.display(),
        source.label
    )))
}

/// Whether `destination` has thread `session_id` among its sessions, asked
/// through `to`: `None` when that can't be told. A thread it finds only in its
/// `archived_sessions/` — where the copy was put — isn't taken.
async fn taken(to: &mut impl CodexTransport, destination: &Home, session_id: &str) -> Option<bool> {
    match read_thread(to, session_id).await {
        Ok(thread) => Some(!is_archived(
            &destination.config_dir,
            thread.path.as_deref(),
        )),
        Err(error) => match resolve_error(&error, session_id, destination) {
            AppError::NotFound(_) => Some(false),
            _ => None,
        },
    }
}

/// The error for `copy`, which `destination` didn't take in, failing with
/// `error`, after setting it aside with [`set_aside`] if it is still there. A
/// place is only named once the copy is known to be there.
fn not_taken(copy: &Path, destination: &Home, error: &CodexRpcError) -> AppError {
    let refused = format!("{} couldn't take it (Codex: {error})", destination.label);
    if !occupied(copy) {
        return AppError::Validation(refused);
    }
    match set_aside(copy) {
        Ok((aside, None)) => AppError::Validation(format!(
            "{refused}. Its copy is set aside in {}",
            aside.display()
        )),
        Ok((aside, Some(left))) => AppError::Validation(format!(
            "{refused}. Its copy is set aside in {}, and is still also at {}",
            aside.display(),
            left.display()
        )),
        Err(_) if !occupied(copy) => AppError::Validation(refused),
        Err(set_aside_error) => AppError::Validation(format!(
            "{refused}. Its copy couldn't be set aside ({set_aside_error}), so it is still in {}",
            copy.display()
        )),
    }
}

/// Move `copy` into [`FAILED_DIR`] beside it, under a name app-server doesn't
/// read as the thread's. Returns where it went, and where it was left too if
/// its old place couldn't be unlinked. Nothing is replaced: a name taken by
/// an earlier failure gets a number.
fn set_aside(copy: &Path) -> std::io::Result<(PathBuf, Option<PathBuf>)> {
    let (Some(folder), Some(name)) = (copy.parent(), copy.file_name()) else {
        return Err(std::io::Error::other("it has no name"));
    };
    let name = name.to_string_lossy();
    let failed = folder.join(FAILED_DIR);
    fs::create_dir_all(&failed)?;
    for count in 1..=100 {
        let aside = match count {
            1 => failed.join(format!("{name}.{FAILED_SUFFIX}")),
            _ => failed.join(format!("{name}.{count}.{FAILED_SUFFIX}")),
        };
        match move_new(copy, &aside) {
            Ok(left) => return Ok((aside, left)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::other("every name for it is taken"))
}

/// The error for a move whose unarchive at `destination` failed with `error`
/// without it being told whether `destination` took the session. Nothing more
/// is done: the session stays in `source`, and `copy` where it is, if it is.
fn undecided(copy: &Path, source: &Home, destination: &Home, error: &CodexRpcError) -> AppError {
    let mut message = format!(
        "Couldn't tell whether {} took it (Codex: {error}). It is still in {}; check {} before \
         moving it again.",
        destination.label, source.label, destination.label
    );
    if occupied(copy) {
        message.push_str(&format!(" Its copy is in {}", copy.display()));
    }
    AppError::Validation(message)
}

#[cfg(test)]
mod tests;
