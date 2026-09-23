//! Codex sessions: the threads a `CODEX_HOME` holds, read through
//! `codex app-server`'s `thread/list`.
//!
//! Each thread is a rollout file, `<CODEX_HOME>/sessions/…/rollout-…-<id>.jsonl`
//! (or under `archived_sessions/` once archived), whose first line is a
//! `session_meta` record naming the client that started it. A process writing
//! to a thread holds `<CODEX_HOME>/thread-writer-locks/<id>.lock`.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use chrono::DateTime;
use serde::Deserialize;
use serde_json::{json, Value};

use super::actions::{ActionCheck, AppToQuit, Checked, SessionAction};
use super::instance::{desktop_label, desktop_pid};
use super::list::{unmovable_reason, Session, SessionKind, SessionState, OPEN_IN_TERMINAL};
use super::{non_blank, Home};
use crate::codex_rpc::{CodexRpc, CodexRpcError, CodexTransport};
use crate::error::{AppError, AppResult};
use crate::launch::process_list;

/// The thread sources listed: the CLI, IDE extensions (the desktop app reports
/// itself as one) and app-server clients. `codex exec` runs and subagents are
/// left out.
const SOURCE_KINDS: [&str; 3] = ["cli", "vscode", "appServer"];

/// How many threads one `thread/list` page asks for.
const PAGE_SIZE: usize = 100;

/// How many threads are listed at most, of the active and of the archived
/// ones each.
const MAX_THREADS: usize = 2000;

/// The `originator` of a rollout the desktop app started.
const DESKTOP_ORIGINATOR: &str = "Codex Desktop";

/// How long after it was written a writer lock counts as held. A process
/// writing to a thread holds its lock file, but the files outlive the
/// processes, so one left behind can only be told from a live one by its age.
const FRESH_LOCK: Duration = Duration::from_secs(10 * 60);

/// Whether each rollout was started by the desktop app, by path. A rollout's
/// first line is written once, when the file is created, and its path names
/// the thread, so the answer never changes for a path.
static DESKTOP_ROLLOUTS: LazyLock<Mutex<HashMap<PathBuf, bool>>> = LazyLock::new(Mutex::default);

/// The fields of a `thread/list` thread read here.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Thread {
    /// The thread id.
    id: String,
    /// The title the user gave the thread.
    name: Option<String>,
    /// Usually the first user message.
    preview: Option<String>,
    /// The folder the thread works in.
    cwd: Option<String>,
    /// When the thread was last updated, in seconds since the epoch.
    #[serde(default)]
    updated_at: i64,
    /// The thread is never written to disk.
    #[serde(default)]
    ephemeral: bool,
    /// The thread that spawned this one, which makes it a subagent.
    parent_thread_id: Option<String>,
    /// The thread's rollout file.
    path: Option<PathBuf>,
    /// What the thread is doing in the app-server that listed it. One started
    /// just to list has loaded no thread, so it reports threads that other
    /// processes have open as `notLoaded`, like any other: their writer locks
    /// tell those apart.
    status: Option<ThreadStatus>,
    /// The thread was listed as archived.
    #[serde(skip)]
    archived: bool,
}

/// A thread's `status`.
#[derive(Debug, Deserialize)]
struct ThreadStatus {
    /// `notLoaded`, `idle`, `systemError` or `active`.
    #[serde(rename = "type")]
    kind: String,
}

/// One page of `thread/list` results.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadPage {
    /// The page's threads, each read on its own so one that can't be read
    /// doesn't lose the page.
    #[serde(default)]
    data: Vec<Value>,
    /// Where the next page starts, or `None` after the last page.
    next_cursor: Option<String>,
}

/// The fields of a rollout's first line read here.
#[derive(Deserialize)]
struct RolloutMeta {
    /// The `session_meta` record's body.
    payload: RolloutMetaPayload,
}

/// The body of a rollout's `session_meta` record.
#[derive(Deserialize)]
struct RolloutMetaPayload {
    /// The client that started the thread: `Codex Desktop`, `codex-tui`, …
    originator: Option<String>,
}

/// The sessions `home` holds, active and archived, most recently used first.
pub async fn list(home: &Home) -> AppResult<Vec<Session>> {
    let mut rpc = CodexRpc::start(&home.config_dir)
        .await
        .map_err(|error| listing_error(&error))?;
    list_with(&mut rpc, home).await
}

/// The sessions `home` holds, most recently used first, read through
/// `transport`.
async fn list_with(transport: &mut impl CodexTransport, home: &Home) -> AppResult<Vec<Session>> {
    let mut threads = Vec::new();
    for archived in [false, true] {
        let listed = list_threads(transport, archived)
            .await
            .map_err(|error| listing_error(&error))?;
        threads.extend(listed);
    }
    let home = home.clone();
    tokio::task::spawn_blocking(move || {
        // Without a process list nothing shows as open in the desktop app,
        // which is only wrong until the next listing.
        let ps_output = process_list().unwrap_or_default();
        let mut sessions = to_sessions(&home, threads, &ps_output, SystemTime::now());
        sessions.sort_by_key(|session| Reverse(session.last_used_at));
        sessions
    })
    .await
    .map_err(|error| AppError::Io(std::io::Error::other(error)))
}

/// The top-level threads listed as `archived` (or not), most recently updated
/// first, page by page up to [`MAX_THREADS`].
async fn list_threads(
    transport: &mut impl CodexTransport,
    archived: bool,
) -> Result<Vec<Thread>, CodexRpcError> {
    let mut threads = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let params = json!({
            "archived": archived,
            "limit": PAGE_SIZE,
            "cursor": cursor,
            "sortKey": "updated_at",
            "sourceKinds": SOURCE_KINDS,
        });
        let page = transport.request("thread/list", params).await?;
        let page: ThreadPage = serde_json::from_value(page)
            .map_err(|error| CodexRpcError::Unexpected(error.to_string()))?;
        if page.data.is_empty() {
            break;
        }
        let listed = page
            .data
            .into_iter()
            .filter_map(|thread| serde_json::from_value::<Thread>(thread).ok())
            .filter(|thread| !thread.ephemeral && thread.parent_thread_id.is_none())
            .map(|thread| Thread { archived, ..thread });
        threads.extend(listed);
        cursor = page.next_cursor;
        if cursor.is_none() || threads.len() >= MAX_THREADS {
            break;
        }
    }
    threads.truncate(MAX_THREADS);
    Ok(threads)
}

/// The rows of `threads` of `home`, given the output of
/// `ps -ax -o pid=,command=`, at `now`.
fn to_sessions(
    home: &Home,
    threads: Vec<Thread>,
    ps_output: &str,
    now: SystemTime,
) -> Vec<Session> {
    let desktop_running = desktop_pid(home, ps_output).is_some();
    threads
        .into_iter()
        .map(|thread| {
            let kind = if thread.path.as_deref().is_some_and(started_in_desktop) {
                SessionKind::Desktop
            } else {
                SessionKind::Cli
            };
            let active = thread
                .status
                .as_ref()
                .is_some_and(|status| status.kind == "active");
            let open =
                !thread.archived && (active || lock_is_fresh(&home.config_dir, &thread.id, now));
            let state = match kind {
                _ if !open => SessionState::Idle,
                SessionKind::Desktop if desktop_running => SessionState::OpenInDesktop,
                _ => SessionState::OpenInTerminal,
            };
            let preview = non_blank(thread.preview);
            let cwd = non_blank(thread.cwd);
            Session {
                id: thread.id,
                kind,
                title: non_blank(thread.name).or_else(|| preview.clone()),
                unmovable_reason: unmovable_reason(home, state, cwd.as_deref()),
                cwd,
                last_prompt: preview,
                last_used_at: DateTime::from_timestamp(thread.updated_at, 0).unwrap_or_default(),
                archived: thread.archived,
                state,
                needs_repair: false,
            }
        })
        .collect()
}

/// The rollout at `path` was started by the desktop app. A compressed rollout
/// isn't decompressed to find out, and one that can't be read isn't, so both
/// count as the CLI's.
fn started_in_desktop(path: &Path) -> bool {
    if path.extension().is_some_and(|extension| extension == "zst") {
        return false;
    }
    let cached = DESKTOP_ROLLOUTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(path)
        .copied();
    if let Some(desktop) = cached {
        return desktop;
    }
    // A rollout being created may not have its first line yet, so one that
    // can't be read isn't cached.
    let Some(originator) = originator(path) else {
        return false;
    };
    let desktop = originator == DESKTOP_ORIGINATOR;
    DESKTOP_ROLLOUTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(path.to_path_buf(), desktop);
    desktop
}

/// The `originator` on the first line of the rollout at `path`.
fn originator(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut line = String::new();
    BufReader::new(file).read_line(&mut line).ok()?;
    serde_json::from_str::<RolloutMeta>(&line)
        .ok()?
        .payload
        .originator
}

/// Thread `id`'s writer lock in `codex_home` was written less than
/// [`FRESH_LOCK`] before `now`.
fn lock_is_fresh(codex_home: &Path, id: &str, now: SystemTime) -> bool {
    let Ok(written) =
        fs::metadata(lock_path(codex_home, id)).and_then(|metadata| metadata.modified())
    else {
        return false;
    };
    written
        .checked_add(FRESH_LOCK)
        .is_none_or(|stale_at| stale_at > now)
}

/// Thread `id`'s writer lock file in `codex_home`.
fn lock_path(codex_home: &Path, id: &str) -> PathBuf {
    codex_home
        .join("thread-writer-locks")
        .join(format!("{id}.lock"))
}

/// The error a failed listing shows as.
fn listing_error(error: &CodexRpcError) -> AppError {
    let message = match error {
        CodexRpcError::NotInstalled => {
            "Codex CLI not found. Install it to see this profile's Codex sessions.".to_string()
        }
        _ => format!("Couldn't read Codex sessions: {error}"),
    };
    AppError::Validation(message)
}

// --- Archive / restore -----------------------------------------------------
//
// A Codex session is archived or restored through `thread/archive` and
// `thread/unarchive` on app-server, never by touching the rollout or the
// state db directly. There is no local record of whether a thread is
// archived the way Claude's desktop record has `isArchived`, and `thread/list`
// has no id filter (paging the whole catalog to find one thread is both
// costly and, without the same `sortKey`/cap as the listing, liable to miss
// a thread the UI shows), so [`check_with`] and [`apply_with`] both resolve
// the thread with a single `thread/read` instead: archived state is read off
// its rollout `path` (under `<config_dir>/archived_sessions/` once archived),
// and a "thread not loaded" error means the id doesn't exist. Right before
// the write, [`apply_with`] re-probes the thread's live status, the writer
// lock and the desktop app again — a live terminal, or the desktop app being
// quit, may have changed things since the check — and every probe fails
// *closed*: anything that can't be told apart from "still open" refuses the
// write rather than risking one under a session that's actually live.

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

/// The `thread` field of a `thread/read` response.
#[derive(Deserialize)]
struct ThreadReadResult {
    /// The thread read.
    thread: Thread,
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
/// status or a held writer lock (Ruling R13 — a listing's mtime-based
/// freshness heuristic, Ruling R12, is too coarse for gating a write: it
/// stays "fresh" for minutes after the process that held it, including the
/// desktop app, is gone), unless the thread was started in the desktop app
/// and that home's instance is what holds it (then it is only `app_to_quit`,
/// as for Claude). Every write needs that instance quit first regardless,
/// since it keeps its own thread catalog.
async fn check_with(
    transport: &mut impl CodexTransport,
    home: &Home,
    session_id: &str,
    action: SessionAction,
    ps_output: &str,
) -> AppResult<Checked<Target>> {
    let thread = read_thread(transport, session_id)
        .await
        .map_err(|error| resolve_error(&error, session_id, home))?;
    let archived = is_archived(&home.config_dir, thread.path.as_deref());
    let kind_desktop = thread.path.as_deref().is_some_and(started_in_desktop);
    let desktop_running = desktop_pid(home, ps_output).is_some();
    let active = thread
        .status
        .as_ref()
        .is_some_and(|status| status.kind == "active");
    let held = held_lock(&home.config_dir, session_id).await?;
    let open = !archived && (active || held);
    let open_in_terminal = open && !(kind_desktop && desktop_running);
    let blocker = match action {
        _ if open_in_terminal => Some(OPEN_IN_TERMINAL),
        SessionAction::Archive if archived => Some(ALREADY_ARCHIVED),
        SessionAction::Restore if !archived => Some(NOT_ARCHIVED),
        _ => None,
    };
    Ok(Checked {
        check: ActionCheck {
            blocker: blocker.map(str::to_string),
            app_to_quit: desktop_running.then(|| AppToQuit::of(home)),
        },
        target: Target {
            id: session_id.to_string(),
        },
    })
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
/// Re-probes right before writing (Ruling R13), in order: the thread's live
/// status, over `transport`; the writer lock, by `lsof`; the desktop instance
/// again. The process probes — the two things that can change from outside
/// this call between the check and here — come last, as close to the write
/// as this can get them. Nothing is written unless all three are clear.
async fn apply_with(
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
        return Err(AppError::Validation(OPEN_IN_TERMINAL.to_string()));
    }
    if held_lock(&home.config_dir, &target.id).await? {
        return Err(AppError::Validation(OPEN_IN_TERMINAL.to_string()));
    }
    if desktop_pid(home, ps_output).is_some() {
        return Err(AppError::Validation(format!(
            "{} is running again — quit it and try again",
            desktop_label(home)
        )));
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
/// since a live terminal may have changed it since.
async fn read_thread(
    transport: &mut impl CodexTransport,
    id: &str,
) -> Result<Thread, CodexRpcError> {
    let response = transport
        .request(
            "thread/read",
            json!({ "threadId": id, "includeTurns": false }),
        )
        .await?;
    let result: ThreadReadResult = serde_json::from_value(response)
        .map_err(|error| CodexRpcError::Unexpected(error.to_string()))?;
    Ok(result.thread)
}

/// The error a failed `thread/read` for `session_id` of `home` becomes.
/// App-server answers a `threadId` it doesn't know with a JSON-RPC error
/// whose message starts "thread not loaded: <id>" (code -32600) — that's the
/// only way this ever fails for an id that's simply wrong, so it becomes
/// [`not_found`]; anything else is [`action_error`].
fn resolve_error(error: &CodexRpcError, session_id: &str, home: &Home) -> AppError {
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
fn is_archived(config_dir: &Path, path: Option<&Path>) -> bool {
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
async fn ps_output() -> AppResult<String> {
    tokio::task::spawn_blocking(process_list)
        .await
        .map_err(|error| AppError::Io(std::io::Error::other(error)))?
}

/// Whether thread `id`'s writer lock in `codex_home` is held right now,
/// resolved off the async runtime thread since it shells out to `lsof`.
async fn held_lock(codex_home: &Path, id: &str) -> AppResult<bool> {
    let codex_home = codex_home.to_path_buf();
    let id = id.to_string();
    let holder = tokio::task::spawn_blocking(move || lock_holder_pid(&codex_home, &id))
        .await
        .map_err(|error| AppError::Io(std::io::Error::other(error)))??;
    Ok(holder.is_some())
}

/// `lsof`'s path: a Tauri app launched from Finder doesn't inherit the shell
/// PATH, so this is absolute rather than relying on one being set.
const LSOF_BINARY: &str = "/usr/sbin/lsof";

/// The pid holding thread `id`'s writer lock in `codex_home`, via `lsof -t`.
/// No lock file means no holder. `lsof` itself failing to run is an error,
/// not a clean bill of health — see [`holder_pid_from_lsof`].
fn lock_holder_pid(codex_home: &Path, id: &str) -> AppResult<Option<i32>> {
    let lock = lock_path(codex_home, id);
    if !lock.exists() {
        return Ok(None);
    }
    let output = Command::new(LSOF_BINARY)
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
        CodexRpcError::NotInstalled => AppError::Validation(
            "Codex CLI not found. Install it to archive or restore this session.".to_string(),
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
    use std::collections::VecDeque;

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use tempfile::tempdir;

    use super::*;
    use crate::app_kind::AppKind;

    const PAGE: &str = include_str!("fixtures/codex-thread-list.json");

    const NAMED: &str = "019dd838-67d9-7112-b0cc-29a785573e73";
    const ACTIVE: &str = "019e1a2b-0000-7000-8000-000000000003";
    const CLI: &str = "019d7637-2fc8-76b3-ba4c-b260a550ae10";

    /// A stand-in for app-server: answers each `thread/list` with the next of
    /// its pages and records the params it was sent.
    struct FakeServer {
        /// The pages still to answer with, per `archived`.
        pages: HashMap<bool, VecDeque<Value>>,
        /// The params of every request, in order.
        requests: Vec<Value>,
    }

    impl FakeServer {
        fn new(active: Vec<Value>, archived: Vec<Value>) -> Self {
            Self {
                pages: HashMap::from([(false, active.into()), (true, archived.into())]),
                requests: Vec::new(),
            }
        }
    }

    #[async_trait]
    impl CodexTransport for FakeServer {
        async fn request(&mut self, method: &str, params: Value) -> Result<Value, CodexRpcError> {
            assert_eq!(method, "thread/list");
            let archived = params["archived"].as_bool().unwrap();
            self.requests.push(params);
            Ok(self
                .pages
                .get_mut(&archived)
                .and_then(VecDeque::pop_front)
                .unwrap_or_else(|| json!({ "data": [], "nextCursor": null })))
        }
    }

    fn page() -> Value {
        serde_json::from_str(PAGE).unwrap()
    }

    fn thread(id: &str) -> Value {
        json!({ "id": id, "preview": "Hi", "cwd": "/work/app", "updatedAt": 1789000000 })
    }

    fn home(root: &Path, stock: bool) -> Home {
        Home {
            id: "personal".to_string(),
            app: AppKind::Codex,
            label: "Personal".to_string(),
            config_dir: root.join("cli-config"),
            gui_data_dir: root.join("gui-data"),
            stock,
        }
    }

    fn write_rollout(root: &Path, id: &str, originator: &str) -> PathBuf {
        let dir = root
            .join("cli-config")
            .join("sessions")
            .join("2026")
            .join("09")
            .join("01");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("rollout-2026-09-01T10-00-00-{id}.jsonl"));
        let meta = json!({
            "type": "session_meta",
            "payload": { "id": id, "originator": originator, "base_instructions": { "text": "…" } },
        });
        fs::write(&path, format!("{meta}\n{{\"type\":\"event_msg\"}}\n")).unwrap();
        path
    }

    fn write_lock(home: &Home, id: &str) {
        let dir = home.config_dir.join("thread-writer-locks");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{id}.lock")), "").unwrap();
    }

    fn running_desktop(home: &Home) -> String {
        format!(
            "  900 /Applications/ChatGPT.app/Contents/MacOS/ChatGPT --user-data-dir={}\n",
            home.gui_data_dir.display()
        )
    }

    fn utc(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    fn threads_of(value: Value, archived: bool) -> Vec<Thread> {
        let page: ThreadPage = serde_json::from_value(value).unwrap();
        page.data
            .into_iter()
            .map(|thread| Thread {
                archived,
                ..serde_json::from_value(thread).unwrap()
            })
            .collect()
    }

    #[tokio::test]
    async fn listing_pages_until_the_cursor_runs_out() {
        let mut server = FakeServer::new(
            vec![
                json!({ "data": [thread("a")], "nextCursor": "page-2" }),
                json!({ "data": [thread("b")], "nextCursor": null }),
            ],
            vec![],
        );

        let threads = list_threads(&mut server, false).await.unwrap();

        let ids: Vec<&str> = threads.iter().map(|thread| thread.id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(
            server.requests,
            [
                json!({
                    "archived": false,
                    "limit": 100,
                    "cursor": null,
                    "sortKey": "updated_at",
                    "sourceKinds": ["cli", "vscode", "appServer"],
                }),
                json!({
                    "archived": false,
                    "limit": 100,
                    "cursor": "page-2",
                    "sortKey": "updated_at",
                    "sourceKinds": ["cli", "vscode", "appServer"],
                }),
            ]
        );
    }

    #[tokio::test]
    async fn listing_stops_at_the_cap() {
        let full_page = json!({
            "data": (0..PAGE_SIZE).map(|index| thread(&format!("t{index}"))).collect::<Vec<_>>(),
            "nextCursor": "more",
        });
        let mut server = FakeServer::new(vec![full_page; 30], vec![]);

        let threads = list_threads(&mut server, false).await.unwrap();

        assert_eq!(threads.len(), MAX_THREADS);
        assert_eq!(server.requests.len(), MAX_THREADS / PAGE_SIZE);
    }

    #[tokio::test]
    async fn listing_stops_on_an_empty_page() {
        let mut server =
            FakeServer::new(vec![json!({ "data": [], "nextCursor": "same" }); 3], vec![]);

        let threads = list_threads(&mut server, false).await.unwrap();

        assert!(threads.is_empty());
        assert_eq!(server.requests.len(), 1);
    }

    #[tokio::test]
    async fn subagent_ephemeral_and_unreadable_threads_are_skipped() {
        let mut listed = page();
        listed["data"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "id": 42 }));
        let mut server = FakeServer::new(vec![listed], vec![]);

        let threads = list_threads(&mut server, false).await.unwrap();

        let ids: Vec<&str> = threads.iter().map(|thread| thread.id.as_str()).collect();
        assert_eq!(ids, [NAMED, ACTIVE, CLI]);
    }

    #[tokio::test]
    async fn a_page_that_isnt_one_fails_the_listing() {
        let mut server = FakeServer::new(vec![json!("nope")], vec![]);

        let listed = list_threads(&mut server, false).await;

        assert!(matches!(listed, Err(CodexRpcError::Unexpected(_))));
    }

    #[tokio::test]
    async fn active_and_archived_threads_are_both_listed_most_recent_first() {
        let root = tempdir().unwrap();
        let mut shelved = thread("shelved");
        shelved["updatedAt"] = json!(1789500000);
        let mut server = FakeServer::new(
            vec![json!({ "data": [thread("live")], "nextCursor": null })],
            vec![json!({ "data": [shelved], "nextCursor": null })],
        );

        let sessions = list_with(&mut server, &home(root.path(), false))
            .await
            .unwrap();

        let listed: Vec<(&str, bool)> = sessions
            .iter()
            .map(|session| (session.id.as_str(), session.archived))
            .collect();
        assert_eq!(listed, [("shelved", true), ("live", false)]);
    }

    #[test]
    fn threads_map_to_sessions() {
        let root = tempdir().unwrap();
        let home = home(root.path(), false);
        let mut listed = page();
        let data = listed["data"].as_array_mut().unwrap();
        data[0]["path"] = json!(write_rollout(root.path(), NAMED, DESKTOP_ORIGINATOR));
        data[1]["path"] = json!(root.path().join(format!("rollout-{ACTIVE}.jsonl.zst")));
        data[2]["path"] = json!(write_rollout(root.path(), CLI, "codex-tui"));
        let mut threads = threads_of(listed, false);
        threads.truncate(3);

        let sessions = to_sessions(&home, threads, "", SystemTime::now());

        assert_eq!(
            sessions,
            [
                Session {
                    id: NAMED.to_string(),
                    kind: SessionKind::Desktop,
                    title: Some("Dark mode".to_string()),
                    cwd: Some("/work/site".to_string()),
                    last_prompt: Some("Add a dark mode toggle".to_string()),
                    last_used_at: utc(1789643901),
                    archived: false,
                    state: SessionState::Idle,
                    needs_repair: false,
                    unmovable_reason: None,
                },
                Session {
                    id: ACTIVE.to_string(),
                    kind: SessionKind::Cli,
                    title: Some("Refactor the parser".to_string()),
                    cwd: Some("/work/app".to_string()),
                    last_prompt: Some("Refactor the parser".to_string()),
                    last_used_at: utc(1779976569),
                    archived: false,
                    state: SessionState::OpenInTerminal,
                    needs_repair: false,
                    unmovable_reason: Some("Close it in the terminal first".to_string()),
                },
                Session {
                    id: CLI.to_string(),
                    kind: SessionKind::Cli,
                    title: Some("Fix the flaky login test".to_string()),
                    cwd: Some("/work/app".to_string()),
                    last_prompt: Some("Fix the flaky login test".to_string()),
                    last_used_at: utc(1775805059),
                    archived: false,
                    state: SessionState::Idle,
                    needs_repair: false,
                    unmovable_reason: None,
                },
            ]
        );
    }

    #[test]
    fn a_blank_name_and_preview_leave_the_session_untitled() {
        let root = tempdir().unwrap();
        let threads = threads_of(
            json!({ "data": [{ "id": "t", "name": " ", "preview": "", "updatedAt": 1 }] }),
            false,
        );

        let sessions = to_sessions(&home(root.path(), false), threads, "", SystemTime::now());

        assert_eq!(
            (sessions[0].title.clone(), sessions[0].last_prompt.clone()),
            (None, None)
        );
    }

    #[test]
    fn a_fresh_writer_lock_opens_a_thread_where_it_was_started() {
        let root = tempdir().unwrap();
        let home = home(root.path(), false);
        let desktop = json!(write_rollout(root.path(), "d", DESKTOP_ORIGINATOR));
        let cli = json!(write_rollout(root.path(), "c", "codex-tui"));
        let listed = json!({ "data": [
            { "id": "d", "path": desktop, "updatedAt": 1 },
            { "id": "c", "path": cli, "updatedAt": 1 },
        ] });
        write_lock(&home, "d");
        write_lock(&home, "c");
        let now = SystemTime::now();
        let states = |ps_output: &str, now: SystemTime, archived: bool| -> Vec<SessionState> {
            to_sessions(&home, threads_of(listed.clone(), archived), ps_output, now)
                .into_iter()
                .map(|session| session.state)
                .collect()
        };

        let with_desktop = states(&running_desktop(&home), now, false);
        let without_desktop = states("", now, false);
        let stale = states(
            &running_desktop(&home),
            now + FRESH_LOCK + Duration::from_secs(1),
            false,
        );
        let archived = states(&running_desktop(&home), now, true);

        assert_eq!(
            with_desktop,
            [SessionState::OpenInDesktop, SessionState::OpenInTerminal]
        );
        assert_eq!(
            without_desktop,
            [SessionState::OpenInTerminal, SessionState::OpenInTerminal]
        );
        assert_eq!(stale, [SessionState::Idle, SessionState::Idle]);
        assert_eq!(archived, [SessionState::Idle, SessionState::Idle]);
    }

    #[test]
    fn a_rollout_that_cant_be_read_counts_as_the_clis() {
        let root = tempdir().unwrap();
        let truncated = root.path().join("rollout-truncated.jsonl");
        fs::write(
            &truncated,
            "{\"type\":\"session_meta\",\"payload\":{\"origin",
        )
        .unwrap();

        assert!(!started_in_desktop(&root.path().join("missing.jsonl")));
        assert!(!started_in_desktop(&truncated));
        assert!(started_in_desktop(&write_rollout(
            root.path(),
            "d",
            DESKTOP_ORIGINATOR
        )));
    }

    #[test]
    fn a_missing_cli_and_a_failed_call_explain_themselves() {
        let missing = listing_error(&CodexRpcError::NotInstalled);
        let failed = listing_error(&CodexRpcError::Closed);

        assert!(
            matches!(missing, AppError::Validation(message) if message.starts_with("Codex CLI not found"))
        );
        assert!(matches!(
            failed,
            AppError::Validation(message)
                if message == "Couldn't read Codex sessions: codex app-server exited before answering"
        ));
    }

    // --- Archive / restore --------------------------------------------------

    /// A stand-in transport that answers each call with `respond`, recording
    /// every call it receives.
    struct ScriptedServer<F> {
        respond: F,
        calls: Vec<(String, Value)>,
    }

    impl<F> ScriptedServer<F>
    where
        F: FnMut(&str, &Value) -> Result<Value, CodexRpcError>,
    {
        fn new(respond: F) -> Self {
            Self {
                respond,
                calls: Vec::new(),
            }
        }
    }

    #[async_trait]
    impl<F> CodexTransport for ScriptedServer<F>
    where
        F: FnMut(&str, &Value) -> Result<Value, CodexRpcError> + Send,
    {
        async fn request(&mut self, method: &str, params: Value) -> Result<Value, CodexRpcError> {
            self.calls.push((method.to_string(), params.clone()));
            (self.respond)(method, &params)
        }
    }

    /// A `thread/read` response for a thread `id`, at `path`, with `status`.
    fn read_response(id: &str, path: Option<&Path>, status: Option<&str>) -> Value {
        json!({
            "thread": {
                "id": id,
                "path": path,
                "status": status.map(|kind| json!({ "type": kind })),
            }
        })
    }

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

        assert_eq!(checked.check.blocker.as_deref(), Some(OPEN_IN_TERMINAL));
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
        // Regression: `check_with` used to gate on the writer lock's mtime
        // (Ruling R12, a listing-only heuristic), which stays "fresh" for
        // minutes after the process that held it — the desktop app, quit by
        // the very action this check is guarding — is gone. Ruling R13's
        // `lsof`-backed `held_lock` reports the lock accurately instead: the
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
            matches!(&applied, Err(AppError::Validation(message)) if message == OPEN_IN_TERMINAL)
        );
        // The fresh status check runs before the lock is re-probed (Ruling
        // R13's order: `thread/read`, then `lsof`, then the desktop app), so
        // it's the only call — the write never happens.
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
            matches!(&applied, Err(AppError::Validation(message)) if message == OPEN_IN_TERMINAL)
        );
        assert_eq!(transport.calls.len(), 1);
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

        assert!(
            matches!(missing, AppError::Validation(message) if message.starts_with("Codex CLI not found"))
        );
        assert!(matches!(rpc, AppError::Validation(message) if message == "Codex: not signed in"));
        assert!(
            matches!(other, AppError::Validation(message) if message == "Codex: codex app-server exited before answering")
        );
    }
}
