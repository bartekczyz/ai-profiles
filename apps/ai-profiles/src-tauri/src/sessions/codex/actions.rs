//! Archiving and restoring a Codex session through app-server.
//!
//! A Codex session is archived or restored through `thread/archive` and
//! `thread/unarchive` on app-server, never by touching the rollout or the
//! state db directly. There is no local record of whether a thread is
//! archived the way Claude's desktop record has `isArchived`, and `thread/list`
//! has no id filter (paging the whole catalog to find one thread is both
//! costly and, without the same `sortKey`/cap as the listing, liable to miss
//! a thread the UI shows), so [`check_with`] and [`apply_with`] both resolve
//! the thread with a single `thread/read` instead: archived state is read off
//! its rollout `path` (under `<config_dir>/archived_sessions/` once archived),
//! and a "thread not loaded" error means the id doesn't exist. Right before
//! the write, [`apply_with`] re-probes the thread's live status, the writer
//! lock and the desktop app again — a live terminal, or the desktop app being
//! quit, may have changed things since the check — and every probe fails
//! *closed*: anything that can't be told apart from "still open" refuses the
//! write rather than risking one under a session that's actually live.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::json;

use super::list::{lock_path, started_in_desktop, Thread};
use crate::codex_rpc::{CodexRpc, CodexRpcError, CodexTransport};
use crate::error::{AppError, AppResult};
use crate::launch::process_list;
use crate::sessions::actions::{blocking, ActionCheck, AppToQuit, Checked, SessionAction};
use crate::sessions::instance::{desktop_pid, running_again};
use crate::sessions::Home;

/// Why a Codex session some process has open can't be written: a terminal,
/// an IDE, or another app-server may hold it, which can't be told apart.
pub(super) const CODEX_HAS_IT_OPEN: &str = "Codex has it open — close it first";

/// Why a Codex session can't be archived again.
const ALREADY_ARCHIVED: &str = "It's already archived";

/// Why a Codex session that isn't archived can't be restored.
const NOT_ARCHIVED: &str = "It isn't archived";

/// What an action on a Codex session is done to: its thread id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The thread's id.
    pub id: String,
}

/// What stands between Codex session `session_id` of `home` and `action`.
pub async fn check(
    home: &Home,
    session_id: &str,
    action: SessionAction,
) -> AppResult<Checked<Target>> {
    let mut rpc = CodexRpc::start(&home.config_dir)
        .await
        .map_err(|error| action_error(&error))?;
    let ps_output = ps_output().await?;
    check_with(&mut rpc, home, session_id, action, &ps_output).await
}

/// [`check`], read through `transport`, given the output of
/// `ps -ax -o pid=,command=`.
///
/// The thread is blocked while it is open outside the desktop app: an active
/// status or a writer lock some process holds open, as `lsof` tells, unless
/// the thread was started in the desktop app and that home's instance is
/// what holds it (then it is only `app_to_quit`, as for Claude). Every write
/// needs that instance quit first regardless, since it keeps its own thread
/// catalog.
///
/// The lock file's age, which a listing goes by, is too coarse to gate a
/// write on: it stays "fresh" for minutes after the process that held it,
/// the desktop app included, is gone.
async fn check_with(
    transport: &mut impl CodexTransport,
    home: &Home,
    session_id: &str,
    action: SessionAction,
    ps_output: &str,
) -> AppResult<Checked<Target>> {
    let (checked, _) = check_thread(transport, home, session_id, action, ps_output).await?;
    Ok(checked)
}

/// [`check_with`], handing back the thread as it was read, too.
pub(super) async fn check_thread(
    transport: &mut impl CodexTransport,
    home: &Home,
    session_id: &str,
    action: SessionAction,
    ps_output: &str,
) -> AppResult<(Checked<Target>, Thread)> {
    let thread = read_thread(transport, session_id)
        .await
        .map_err(|error| resolve_error(&error, session_id, home))?;
    let (config_dir, path) = (home.config_dir.clone(), thread.path.clone());
    let (archived, kind_desktop) = blocking(move || {
        let path = path.as_deref();
        Ok((
            is_archived(&config_dir, path),
            path.is_some_and(started_in_desktop),
        ))
    })
    .await?;
    let desktop_running = desktop_pid(home, ps_output).is_some();
    let active = thread
        .status
        .as_ref()
        .is_some_and(|status| status.kind == "active");
    let held = held_lock(&home.config_dir, session_id).await?;
    let open = !archived && (active || held);
    let open_elsewhere = open && !(kind_desktop && desktop_running);
    let blocker = match action {
        _ if open_elsewhere => Some(CODEX_HAS_IT_OPEN),
        SessionAction::Archive if archived => Some(ALREADY_ARCHIVED),
        SessionAction::Restore if !archived => Some(NOT_ARCHIVED),
        _ => None,
    };
    let checked = Checked {
        check: ActionCheck {
            blocker: blocker.map(str::to_string),
            app_to_quit: desktop_running.then(|| AppToQuit::of(home)),
        },
        target: Target {
            id: session_id.to_string(),
        },
    };
    Ok((checked, thread))
}

/// Do `action` to Codex session `target` of `home`.
pub async fn apply(home: &Home, target: Target, action: SessionAction) -> AppResult<()> {
    let mut rpc = CodexRpc::start(&home.config_dir)
        .await
        .map_err(|error| action_error(&error))?;
    let ps_output = ps_output().await?;
    apply_with(&mut rpc, home, &target, action, &ps_output).await
}

/// [`apply`], written through `transport`, given the output of
/// `ps -ax -o pid=,command=`.
///
/// Codex can open the thread between the check and the write, so this
/// probes again right before writing, in order: the thread's live
/// status, over `transport`; the writer lock, by `lsof`; the desktop instance
/// again. The process probes — the two things that can change from outside
/// this call between the check and here — come last, as close to the write
/// as this can get them. Nothing is written unless all three are clear.
pub(super) async fn apply_with(
    transport: &mut impl CodexTransport,
    home: &Home,
    target: &Target,
    action: SessionAction,
    ps_output: &str,
) -> AppResult<()> {
    let thread = read_thread(transport, &target.id)
        .await
        .map_err(|error| resolve_error(&error, &target.id, home))?;
    let active = thread
        .status
        .as_ref()
        .is_some_and(|status| status.kind == "active");
    if active {
        return Err(AppError::Validation(CODEX_HAS_IT_OPEN.to_string()));
    }
    if held_lock(&home.config_dir, &target.id).await? {
        return Err(AppError::Validation(CODEX_HAS_IT_OPEN.to_string()));
    }
    if desktop_pid(home, ps_output).is_some() {
        return Err(running_again(home));
    }
    let method = match action {
        SessionAction::Archive => "thread/archive",
        SessionAction::Restore => "thread/unarchive",
    };
    transport
        .request(method, json!({ "threadId": target.id }))
        .await
        .map_err(|error| action_error(&error))?;
    Ok(())
}

/// Thread `id`, via a fresh `thread/read` — never a listing taken earlier,
/// since a live terminal may have changed it since — read exactly: what
/// comes back decides whether a write is safe, so a status or path that
/// can't be read fails it.
pub(super) async fn read_thread(
    transport: &mut impl CodexTransport,
    id: &str,
) -> Result<Thread, CodexRpcError> {
    let response = transport
        .request(
            "thread/read",
            json!({ "threadId": id, "includeTurns": false }),
        )
        .await?;
    let thread = response
        .get("thread")
        .ok_or_else(|| CodexRpcError::Unexpected(format!("no thread in {response}")))?;
    Thread::read_exactly(thread).map_err(CodexRpcError::Unexpected)
}

/// The error a failed `thread/read` for `session_id` of `home` becomes.
/// App-server answers a `threadId` it doesn't know with a JSON-RPC error
/// whose message starts "thread not loaded: <id>" (code -32600) — that's the
/// only way this ever fails for an id that's simply wrong, so it becomes
/// [`not_found`]; anything else is [`action_error`].
pub(super) fn resolve_error(error: &CodexRpcError, session_id: &str, home: &Home) -> AppError {
    if let CodexRpcError::Rpc(message) = error {
        if message.starts_with("thread not loaded:") {
            return not_found(session_id, home);
        }
    }
    action_error(error)
}

/// Whether `path` — a thread's rollout file — sits under `config_dir`'s
/// `archived_sessions` folder. Both sides are canonicalized before comparing:
/// app-server resolves symlinks in the paths it returns (macOS's `/tmp` is
/// `/private/tmp`), so a plain prefix check on an unresolved `config_dir`
/// would miss every archived thread. A path that can't be canonicalized (the
/// rollout, or the folder, doesn't exist) is compared as given instead of
/// failing outright — good enough for "definitely not under there".
pub(super) fn is_archived(config_dir: &Path, path: Option<&Path>) -> bool {
    let Some(path) = path else {
        return false;
    };
    let archived_dir = config_dir.join("archived_sessions");
    let canonical_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let canonical_archived_dir = fs::canonicalize(&archived_dir).unwrap_or(archived_dir);
    canonical_path.starts_with(canonical_archived_dir)
}

/// The output of `ps -ax -o pid=,command=`, off the async runtime thread. A
/// failed `ps` fails the action closed — unlike listing's callers, which
/// `unwrap_or_default` since a missed "open in desktop" badge there is only
/// stale for a moment, this decides whether a write is safe to make.
pub(super) async fn ps_output() -> AppResult<String> {
    blocking(process_list).await
}

/// Whether thread `id`'s writer lock in `codex_home` is held right now,
/// resolved off the async runtime thread since it shells out to `lsof`.
pub(super) async fn held_lock(codex_home: &Path, id: &str) -> AppResult<bool> {
    let codex_home = codex_home.to_path_buf();
    let id = id.to_string();
    let holder = blocking(move || lock_holder_pid(&codex_home, &id)).await?;
    Ok(holder.is_some())
}

/// `lsof`'s path: a Tauri app launched from Finder doesn't inherit the shell
/// PATH, so this is absolute rather than relying on one being set.
const LSOF_BINARY: &str = "/usr/sbin/lsof";

/// The pid holding thread `id`'s writer lock in `codex_home`, via `lsof -t`.
/// No lock file means no holder. `-w` keeps `lsof`'s warnings, such as about
/// a network volume it can't look into, off stderr, where they would read as
/// a failure. `lsof` itself failing to run is an error,
/// not a clean bill of health — see [`holder_pid_from_lsof`].
fn lock_holder_pid(codex_home: &Path, id: &str) -> AppResult<Option<i32>> {
    lock_holder_pid_with(Path::new(LSOF_BINARY), codex_home, id)
}

/// [`lock_holder_pid`], asking the `lsof` at `lsof`.
fn lock_holder_pid_with(lsof: &Path, codex_home: &Path, id: &str) -> AppResult<Option<i32>> {
    let lock = lock_path(codex_home, id);
    if !lock.exists() {
        return Ok(None);
    }
    let output = Command::new(lsof)
        .arg("-w")
        .arg("-t")
        .arg(&lock)
        .output()
        .map_err(|_| lsof_error())?;
    holder_pid_from_lsof(&output)
}

/// [`lock_holder_pid`]'s decision from `lsof -t`'s exit status and output. A
/// clean run (exit 0) names the holder on stdout, if there is one; `lsof`'s
/// own way of saying nothing has the file open is exit 1 with nothing on
/// stdout *or* stderr. Anything else — a permissions error, `lsof` choking on
/// the path — can't be told apart from a real holder, so this fails closed
/// with an error rather than reporting no holder.
fn holder_pid_from_lsof(output: &std::process::Output) -> AppResult<Option<i32>> {
    if output.status.success() {
        return Ok(pid_from_lsof_stdout(&output.stdout));
    }
    if output.status.code() == Some(1) && output.stdout.is_empty() && output.stderr.is_empty() {
        return Ok(None);
    }
    Err(lsof_error())
}

/// The first pid on `lsof -t`'s stdout, if any. Anything that isn't a pid —
/// empty output included — reads as none.
fn pid_from_lsof_stdout(stdout: &[u8]) -> Option<i32> {
    std::str::from_utf8(stdout)
        .ok()?
        .lines()
        .next()?
        .trim()
        .parse()
        .ok()
}

/// The error checking or writing a lock fails with when `lsof` can't be
/// asked or its answer can't be read.
fn lsof_error() -> AppError {
    AppError::Validation("Couldn't check whether Codex has it open".to_string())
}

/// The error an archive or restore call fails with. A JSON-RPC error's own
/// message reaches the user verbatim, prefixed so its source is clear.
fn action_error(error: &CodexRpcError) -> AppError {
    match error {
        CodexRpcError::NotInstalled => AppError::NotInstalled(
            "Install the Codex CLI to archive or restore this session".to_string(),
        ),
        _ => AppError::Validation(format!("Codex: {error}")),
    }
}

/// The error for a thread `home` doesn't have.
fn not_found(session_id: &str, home: &Home) -> AppError {
    AppError::NotFound(format!("session {session_id} not found in {}", home.label))
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use serde_json::{json, Value};
    use tempfile::tempdir;

    use super::*;
    use crate::sessions::codex::fakes::{
        home, read_response, running_desktop, write_lock, write_rollout, ScriptedServer,
    };
    use crate::sessions::codex::list::DESKTOP_ORIGINATOR;

    /// A path under `home`'s archived-sessions folder, with a file actually
    /// there so `is_archived`'s canonicalization has something to resolve.
    fn archived_rollout_path(home: &Home, id: &str) -> PathBuf {
        let dir = home.config_dir.join("archived_sessions");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{id}.jsonl"));
        fs::write(&path, "").unwrap();
        path
    }

    /// A `ScriptedServer` that answers a single `thread/read` with `response`
    /// and panics on anything else.
    fn reads_as(
        response: Value,
    ) -> ScriptedServer<impl FnMut(&str, &Value) -> Result<Value, CodexRpcError>> {
        ScriptedServer::new(move |method, _| {
            assert_eq!(method, "thread/read");
            Ok(response.clone())
        })
    }

    #[tokio::test]
    async fn a_thread_open_outside_the_desktop_app_blocks_archiving() {
        let root = tempdir().unwrap();
        let home = home(root.path(), false);
        let cli = write_rollout(root.path(), "t", "codex-tui");
        let mut transport = reads_as(read_response("t", Some(&cli), Some("active")));

        let checked = check_with(&mut transport, &home, "t", SessionAction::Archive, "")
            .await
            .unwrap();

        assert_eq!(checked.check.blocker.as_deref(), Some(CODEX_HAS_IT_OPEN));
        assert_eq!(checked.check.app_to_quit, None);
        assert_eq!(
            checked.target,
            Target {
                id: "t".to_string()
            }
        );
    }

    #[tokio::test]
    async fn a_thread_the_running_desktop_app_holds_is_not_blocked() {
        let root = tempdir().unwrap();
        let home = home(root.path(), false);
        let desktop = write_rollout(root.path(), "t", DESKTOP_ORIGINATOR);
        let mut transport = reads_as(read_response("t", Some(&desktop), Some("active")));

        let checked = check_with(
            &mut transport,
            &home,
            "t",
            SessionAction::Archive,
            &running_desktop(&home),
        )
        .await
        .unwrap();

        assert_eq!(checked.check.blocker, None);
        assert_eq!(
            checked.check.app_to_quit,
            Some(AppToQuit {
                home_id: "personal".to_string(),
                label: "ChatGPT (Personal)".to_string(),
            })
        );
    }

    #[tokio::test]
    async fn quitting_the_desktop_app_is_offered_whenever_it_runs() {
        let root = tempdir().unwrap();
        let home = home(root.path(), false);
        let mut transport = reads_as(read_response("t", None, Some("idle")));

        let checked = check_with(
            &mut transport,
            &home,
            "t",
            SessionAction::Archive,
            &running_desktop(&home),
        )
        .await
        .unwrap();

        assert_eq!(checked.check.blocker, None);
        assert!(checked.check.app_to_quit.is_some());
    }

    #[tokio::test]
    async fn quitting_the_desktop_app_clears_the_block_even_though_the_lock_file_is_still_fresh() {
        // Regression: `check_with` used to gate on the writer lock's mtime,
        // which only suits a listing: it stays "fresh" for minutes after the
        // process that held it — the desktop app, quit by the very action
        // this check is guarding — is gone. The `lsof`-backed `held_lock`
        // reports the lock accurately instead: the
        // file exists here, with a brand new mtime, but nothing holds it
        // open, so neither check should block.
        let root = tempdir().unwrap();
        let home = home(root.path(), false);
        let desktop = write_rollout(root.path(), "t", DESKTOP_ORIGINATOR);
        write_lock(&home, "t");
        let response = read_response("t", Some(&desktop), Some("idle"));
        let mut transport = ScriptedServer::new(move |method, _| {
            assert_eq!(method, "thread/read");
            Ok(response.clone())
        });

        let while_running = check_with(
            &mut transport,
            &home,
            "t",
            SessionAction::Archive,
            &running_desktop(&home),
        )
        .await
        .unwrap();
        let after_quit = check_with(&mut transport, &home, "t", SessionAction::Archive, "")
            .await
            .unwrap();

        assert_eq!(while_running.check.blocker, None);
        assert!(while_running.check.app_to_quit.is_some());
        assert_eq!(after_quit.check.blocker, None);
        assert_eq!(after_quit.check.app_to_quit, None);
    }

    #[tokio::test]
    async fn an_already_archived_thread_cant_be_archived_again() {
        let root = tempdir().unwrap();
        let home = home(root.path(), false);
        let archived_path = archived_rollout_path(&home, "t");
        let mut transport = reads_as(read_response("t", Some(&archived_path), None));

        let checked = check_with(&mut transport, &home, "t", SessionAction::Archive, "")
            .await
            .unwrap();

        assert_eq!(checked.check.blocker.as_deref(), Some(ALREADY_ARCHIVED));
    }

    #[tokio::test]
    async fn a_thread_that_isnt_archived_cant_be_restored() {
        let root = tempdir().unwrap();
        let home = home(root.path(), false);
        let cli = write_rollout(root.path(), "t", "codex-tui");
        let mut transport = reads_as(read_response("t", Some(&cli), None));

        let checked = check_with(&mut transport, &home, "t", SessionAction::Restore, "")
            .await
            .unwrap();

        assert_eq!(checked.check.blocker.as_deref(), Some(NOT_ARCHIVED));
    }

    #[tokio::test]
    async fn a_thread_app_server_doesnt_know_is_not_found() {
        let root = tempdir().unwrap();
        let home = home(root.path(), false);
        let mut transport = ScriptedServer::new(|method, _| {
            assert_eq!(method, "thread/read");
            Err(CodexRpcError::Rpc("thread not loaded: missing".to_string()))
        });

        let checked =
            check_with(&mut transport, &home, "missing", SessionAction::Archive, "").await;

        assert!(matches!(checked, Err(AppError::NotFound(_))));
    }

    #[tokio::test]
    async fn a_failed_check_call_surfaces_the_rpc_message_prefixed() {
        let root = tempdir().unwrap();
        let home = home(root.path(), false);
        let mut transport =
            ScriptedServer::new(|_, _| Err(CodexRpcError::Rpc("not signed in".to_string())));

        let checked = check_with(&mut transport, &home, "t", SessionAction::Archive, "").await;

        assert!(
            matches!(&checked, Err(AppError::Validation(message)) if message == "Codex: not signed in")
        );
    }

    #[test]
    fn resolve_error_maps_thread_not_loaded_to_not_found_and_leaves_everything_else_alone() {
        let root = tempdir().unwrap();
        let home = home(root.path(), false);

        let not_loaded = resolve_error(
            &CodexRpcError::Rpc("thread not loaded: t".to_string()),
            "t",
            &home,
        );
        let other = resolve_error(&CodexRpcError::Rpc("not signed in".to_string()), "t", &home);

        assert!(matches!(not_loaded, AppError::NotFound(_)));
        assert!(
            matches!(other, AppError::Validation(message) if message == "Codex: not signed in")
        );
    }

    #[test]
    fn archived_state_is_told_by_the_rollout_path_even_through_a_symlinked_temp_dir() {
        let root = tempdir().unwrap();
        let config_dir = root.path().join("cli-config");
        let archived_dir = config_dir.join("archived_sessions");
        fs::create_dir_all(&archived_dir).unwrap();
        let archived_path = archived_dir.join("t.jsonl");
        fs::write(&archived_path, "").unwrap();
        // App-server hands back an already-resolved path (macOS's real /tmp
        // is /private/tmp); `config_dir` here is the tempdir's own,
        // unresolved path, so this only matches if both sides are
        // canonicalized before comparing.
        let canonical_archived_path = fs::canonicalize(&archived_path).unwrap();
        let active_dir = config_dir.join("sessions");
        fs::create_dir_all(&active_dir).unwrap();
        let active_path = active_dir.join("c.jsonl");
        fs::write(&active_path, "").unwrap();

        assert!(is_archived(&config_dir, Some(&canonical_archived_path)));
        assert!(!is_archived(&config_dir, Some(&active_path)));
        assert!(!is_archived(&config_dir, None));
    }

    #[tokio::test]
    async fn archiving_an_idle_thread_calls_thread_archive_with_its_id() {
        let root = tempdir().unwrap();
        let home = home(root.path(), false);
        let target = Target {
            id: "t".to_string(),
        };
        let mut transport = ScriptedServer::new(|method, _| match method {
            "thread/read" => Ok(json!({ "thread": { "id": "t", "status": { "type": "idle" } } })),
            "thread/archive" => Ok(json!({})),
            other => panic!("unexpected call: {other}"),
        });

        apply_with(&mut transport, &home, &target, SessionAction::Archive, "")
            .await
            .unwrap();

        assert_eq!(
            transport.calls,
            [
                (
                    "thread/read".to_string(),
                    json!({ "threadId": "t", "includeTurns": false })
                ),
                ("thread/archive".to_string(), json!({ "threadId": "t" })),
            ]
        );
    }

    #[tokio::test]
    async fn restoring_calls_thread_unarchive() {
        let root = tempdir().unwrap();
        let home = home(root.path(), false);
        let target = Target {
            id: "t".to_string(),
        };
        let mut transport = ScriptedServer::new(|method, _| match method {
            "thread/read" => Ok(json!({ "thread": { "id": "t", "status": { "type": "idle" } } })),
            "thread/unarchive" => Ok(json!({})),
            other => panic!("unexpected call: {other}"),
        });

        apply_with(&mut transport, &home, &target, SessionAction::Restore, "")
            .await
            .unwrap();

        assert_eq!(transport.calls[1].0, "thread/unarchive");
    }

    #[tokio::test]
    async fn nothing_is_written_while_the_writer_lock_is_held() {
        let root = tempdir().unwrap();
        let home = home(root.path(), false);
        let lock_dir = home.config_dir.join("thread-writer-locks");
        fs::create_dir_all(&lock_dir).unwrap();
        // Holding the lock file open in this process is enough for `lsof -t`
        // to name this process as a holder, with no other process needed.
        let held = File::create(lock_dir.join("t.lock")).unwrap();
        let target = Target {
            id: "t".to_string(),
        };
        let mut transport = ScriptedServer::new(|method, _| match method {
            "thread/read" => Ok(json!({ "thread": { "id": "t", "status": { "type": "idle" } } })),
            other => panic!("unexpected call: {other}"),
        });

        let applied = apply_with(&mut transport, &home, &target, SessionAction::Archive, "").await;

        drop(held);
        assert!(
            matches!(&applied, Err(AppError::Validation(message)) if message == CODEX_HAS_IT_OPEN)
        );
        // The fresh status check runs before the lock is probed again
        // (`thread/read`, then `lsof`, then the desktop app), so it's the
        // only call — the write never happens.
        assert_eq!(transport.calls.len(), 1);
        assert_eq!(transport.calls[0].0, "thread/read");
    }

    #[tokio::test]
    async fn nothing_is_written_while_the_desktop_app_runs_again() {
        let root = tempdir().unwrap();
        let home = home(root.path(), false);
        let target = Target {
            id: "t".to_string(),
        };
        let mut transport = ScriptedServer::new(|method, _| match method {
            "thread/read" => Ok(json!({ "thread": { "id": "t", "status": { "type": "idle" } } })),
            other => panic!("unexpected call: {other}"),
        });

        let applied = apply_with(
            &mut transport,
            &home,
            &target,
            SessionAction::Archive,
            &running_desktop(&home),
        )
        .await;

        assert!(matches!(
            &applied,
            Err(AppError::Validation(message))
                if message == "ChatGPT (Personal) is running again — quit it and try again"
        ));
        assert_eq!(transport.calls.len(), 1);
        assert_eq!(transport.calls[0].0, "thread/read");
    }

    #[tokio::test]
    async fn nothing_is_written_when_a_fresh_check_finds_the_thread_active() {
        let root = tempdir().unwrap();
        let home = home(root.path(), false);
        let target = Target {
            id: "t".to_string(),
        };
        let mut transport = ScriptedServer::new(|method, _| match method {
            "thread/read" => Ok(json!({ "thread": { "id": "t", "status": { "type": "active" } } })),
            other => panic!("unexpected call: {other}"),
        });

        let applied = apply_with(&mut transport, &home, &target, SessionAction::Archive, "").await;

        assert!(
            matches!(&applied, Err(AppError::Validation(message)) if message == CODEX_HAS_IT_OPEN)
        );
        assert_eq!(transport.calls.len(), 1);
    }

    #[tokio::test]
    async fn nothing_is_written_when_the_threads_status_or_path_cant_be_read() {
        let root = tempdir().unwrap();
        let home = home(root.path(), false);
        let target = Target {
            id: "t".to_string(),
        };
        let unreadable = [
            json!({ "id": "t", "status": { "kind": "active" } }),
            json!({ "id": "t", "status": 3 }),
            json!({ "id": "t", "status": { "type": "idle" }, "path": ["sessions"] }),
        ];

        for thread in unreadable {
            let mut transport = ScriptedServer::new(move |method, _| match method {
                "thread/read" => Ok(json!({ "thread": thread.clone() })),
                other => panic!("unexpected call: {other}"),
            });

            let checked = check_with(&mut transport, &home, "t", SessionAction::Archive, "").await;
            let applied =
                apply_with(&mut transport, &home, &target, SessionAction::Archive, "").await;

            assert!(
                matches!(&checked, Err(AppError::Validation(message)) if message.starts_with("Codex: unexpected answer")),
                "{checked:?}"
            );
            assert!(
                matches!(&applied, Err(AppError::Validation(message)) if message.starts_with("Codex: unexpected answer")),
                "{applied:?}"
            );
            assert_eq!(transport.calls.len(), 2);
        }
    }

    #[tokio::test]
    async fn a_failed_write_call_surfaces_the_rpc_message_prefixed() {
        let root = tempdir().unwrap();
        let home = home(root.path(), false);
        let target = Target {
            id: "t".to_string(),
        };
        let mut transport = ScriptedServer::new(|method, _| match method {
            "thread/read" => Ok(json!({ "thread": { "id": "t", "status": { "type": "idle" } } })),
            "thread/archive" => Err(CodexRpcError::Rpc("write conflict".to_string())),
            other => panic!("unexpected call: {other}"),
        });

        let applied = apply_with(&mut transport, &home, &target, SessionAction::Archive, "").await;

        assert!(
            matches!(&applied, Err(AppError::Validation(message)) if message == "Codex: write conflict")
        );
    }

    /// A fabricated `lsof` result: `code` as its exit status, `stdout` and
    /// `stderr` as given. No real process is spawned.
    fn lsof_output(code: i32, stdout: &[u8], stderr: &[u8]) -> std::process::Output {
        std::process::Output {
            status: std::os::unix::process::ExitStatusExt::from_raw(code << 8),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }

    #[test]
    fn holder_pid_from_lsof_reads_a_clean_runs_first_pid_or_none() {
        assert_eq!(
            holder_pid_from_lsof(&lsof_output(0, b"1234\n", b"")).unwrap(),
            Some(1234)
        );
        assert_eq!(
            holder_pid_from_lsof(&lsof_output(0, b"", b"")).unwrap(),
            None
        );
        assert_eq!(
            holder_pid_from_lsof(&lsof_output(0, b"not-a-pid\n", b"")).unwrap(),
            None
        );
    }

    #[test]
    fn holder_pid_from_lsof_reads_exit_1_with_nothing_on_stdout_or_stderr_as_no_holder() {
        assert_eq!(
            holder_pid_from_lsof(&lsof_output(1, b"", b"")).unwrap(),
            None
        );
    }

    #[test]
    fn holder_pid_from_lsof_fails_closed_on_anything_it_cant_read_as_a_clean_no_holder() {
        assert!(
            holder_pid_from_lsof(&lsof_output(1, b"", b"lsof: WARNING: can't stat\n")).is_err()
        );
        assert!(holder_pid_from_lsof(&lsof_output(1, b"1234\n", b"")).is_err());
        assert!(holder_pid_from_lsof(&lsof_output(2, b"", b"")).is_err());
    }

    /// An `lsof` stand-in at `<root>/lsof` that finds no holder and, unless
    /// passed `-w`, warns on stderr about a volume it can't look into, as the
    /// real one does with an unreachable network mount.
    fn warning_lsof(root: &Path) -> PathBuf {
        let path = root.join("lsof");
        fs::write(
            &path,
            "#!/bin/sh
             for arg in \"$@\"; do [ \"$arg\" = -w ] && exit 1; done
             echo \"lsof: WARNING: can't stat() smbfs file system /Volumes/share\" >&2
             exit 1
",
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn a_harmless_lsof_warning_is_not_read_as_a_holder() {
        let root = tempdir().unwrap();
        let codex_home = root.path().join("codex");
        let lock = lock_path(&codex_home, "t");
        fs::create_dir_all(lock.parent().unwrap()).unwrap();
        File::create(&lock).unwrap();
        let lsof = warning_lsof(root.path());

        assert_eq!(lock_holder_pid_with(&lsof, &codex_home, "t").unwrap(), None);
    }

    #[test]
    fn a_missing_lock_file_has_no_holder() {
        let root = tempdir().unwrap();

        assert_eq!(lock_holder_pid(root.path(), "t").unwrap(), None);
    }

    #[test]
    fn action_error_explains_a_missing_cli_and_surfaces_an_rpc_message_prefixed() {
        let missing = action_error(&CodexRpcError::NotInstalled);
        let rpc = action_error(&CodexRpcError::Rpc("not signed in".to_string()));
        let other = action_error(&CodexRpcError::Closed);

        assert!(matches!(
            missing,
            AppError::NotInstalled(message)
                if message == "Install the Codex CLI to archive or restore this session"
        ));
        assert!(matches!(rpc, AppError::Validation(message) if message == "Codex: not signed in"));
        assert!(
            matches!(other, AppError::Validation(message) if message == "Codex: codex app-server exited before answering")
        );
    }
}
