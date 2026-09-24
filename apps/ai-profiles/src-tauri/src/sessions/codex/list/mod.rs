//! Listing a `CODEX_HOME`'s threads through `codex app-server`'s `thread/list`.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::fs::{self, File};
use std::future::Future;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use chrono::DateTime;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::codex_rpc::{CodexRpc, CodexRpcError, CodexTransport};
use crate::error::{AppError, AppResult};
use crate::launch::process_list;
use crate::sessions::instance::desktop_pid;
use crate::sessions::list::{unmovable_reason, Session, SessionKind, SessionState};
use crate::sessions::{non_blank, Home};

/// The thread sources listed: the CLI, IDE extensions (the desktop app reports
/// itself as one) and app-server clients. `codex exec` runs and subagents are
/// left out.
const SOURCE_KINDS: [&str; 3] = ["cli", "vscode", "appServer"];

/// How many threads one `thread/list` page asks for.
const PAGE_SIZE: usize = 100;

/// How many threads are listed at most, of the active and of the archived
/// ones each.
const MAX_THREADS: usize = 2000;

/// How many `thread/list` pages are read at most, of the active and of the
/// archived threads each.
const MAX_PAGES: usize = MAX_THREADS / PAGE_SIZE + 2;

/// How long listing a home's threads may take, all pages of both the active
/// and the archived ones.
const LISTING_TIMEOUT: Duration = Duration::from_secs(20);

/// The `originator` of a rollout the desktop app started.
pub(super) const DESKTOP_ORIGINATOR: &str = "Codex Desktop";

/// How long after it was written a writer lock counts as held. A process
/// writing to a thread holds its lock file, but the files outlive the
/// processes, so one left behind can only be told from a live one by its age.
const FRESH_LOCK: Duration = Duration::from_secs(10 * 60);

/// Whether each rollout was started by the desktop app, by path. A rollout's
/// first line is written once, when the file is created, and its path names
/// the thread, so the answer never changes for a path.
static DESKTOP_ROLLOUTS: LazyLock<Mutex<HashMap<PathBuf, bool>>> = LazyLock::new(Mutex::default);

/// The fields of a `thread/list` or `thread/read` thread read here.
#[derive(Debug)]
pub(super) struct Thread {
    /// The thread id.
    id: String,
    /// The title the user gave the thread.
    name: Option<String>,
    /// Usually the first user message.
    preview: Option<String>,
    /// The folder the thread works in.
    cwd: Option<String>,
    /// When the thread was last updated, in seconds since the epoch.
    updated_at: i64,
    /// The thread is never written to disk.
    ephemeral: bool,
    /// The thread that spawned this one, which makes it a subagent.
    parent_thread_id: Option<String>,
    /// The thread's rollout file.
    pub(super) path: Option<PathBuf>,
    /// What the thread is doing in the app-server that listed it. One started
    /// just to list has loaded no thread, so it reports threads that other
    /// processes have open as `notLoaded`, like any other: their writer locks
    /// tell those apart.
    pub(super) status: Option<ThreadStatus>,
    /// The thread was listed as archived.
    archived: bool,
}

impl Thread {
    /// The thread `value` describes, read field by field: a field that is
    /// missing or of a type it can't be read as is left empty, so one field
    /// app-server changes doesn't lose the thread. `None` without an id.
    ///
    /// Its update time falls back to its recency, then its creation time.
    pub(super) fn read(value: &Value) -> Option<Self> {
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        let seconds = |key: &str| value.get(key).and_then(timestamp);
        Some(Thread {
            id: text("id")?,
            name: text("name"),
            preview: text("preview"),
            cwd: text("cwd"),
            updated_at: seconds("updatedAt")
                .or_else(|| seconds("recencyAt"))
                .or_else(|| seconds("createdAt"))
                .unwrap_or_default(),
            ephemeral: value
                .get("ephemeral")
                .and_then(Value::as_bool)
                .unwrap_or_default(),
            parent_thread_id: text("parentThreadId"),
            path: text("path").map(PathBuf::from),
            status: value.get("status").and_then(ThreadStatus::read),
            archived: false,
        })
    }

    /// [`Thread::read`], for a thread about to be written to: a `status` or
    /// `path` that is there but can't be read fails it rather than reading
    /// as none, since a thread taken for idle, or for having no file, could
    /// let a write through under one that is live.
    pub(super) fn read_exactly(value: &Value) -> Result<Self, String> {
        let present = |key: &str| value.get(key).filter(|field| !field.is_null());
        if let Some(status) = present("status") {
            if ThreadStatus::read(status).is_none() {
                return Err(format!("unreadable thread status {status}"));
            }
        }
        if let Some(path) = present("path") {
            if !path.is_string() {
                return Err(format!("unreadable thread path {path}"));
            }
        }
        Self::read(value).ok_or_else(|| format!("no thread id in {value}"))
    }
}

/// A time in seconds since the epoch: a number, or text holding one or an
/// RFC 3339 date.
fn timestamp(value: &Value) -> Option<i64> {
    if value.is_number() {
        return value
            .as_i64()
            .or_else(|| value.as_f64().map(|seconds| seconds as i64));
    }
    let text = value.as_str()?.trim();
    text.parse().ok().or_else(|| {
        DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|date| date.timestamp())
    })
}

/// A thread's `status`.
#[derive(Debug)]
pub(super) struct ThreadStatus {
    /// `notLoaded`, `idle`, `systemError` or `active`.
    pub(super) kind: String,
}

impl ThreadStatus {
    /// The status `value` describes: an object whose `type` names it, or
    /// just its name.
    fn read(value: &Value) -> Option<Self> {
        let kind = value.get("type").unwrap_or(value).as_str()?;
        Some(ThreadStatus {
            kind: kind.to_string(),
        })
    }
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
    // Without a process list nothing shows as open in the desktop app, which
    // is only wrong until the next listing.
    let processes = || process_list().unwrap_or_default();
    list_with(
        CodexRpc::start(&home.config_dir),
        home,
        LISTING_TIMEOUT,
        processes,
    )
    .await
}

/// The sessions `home` holds, most recently used first, read through the
/// transport `start` gives within `timeout`, with `processes` giving the
/// output of `ps -ax -o pid=,command=`.
async fn list_with<T: CodexTransport>(
    start: impl Future<Output = Result<T, CodexRpcError>>,
    home: &Home,
    timeout: Duration,
    processes: impl FnOnce() -> String + Send + 'static,
) -> AppResult<Vec<Session>> {
    // The transport is dropped, stopping app-server, as soon as the threads
    // are listed: working out the rows needs no more of it.
    let listing = async {
        let mut transport = start.await?;
        let mut threads = list_threads(&mut transport, false).await?;
        threads.extend(list_threads(&mut transport, true).await?);
        Ok(threads)
    };
    let threads = tokio::time::timeout(timeout, listing)
        .await
        .map_err(|_| listing_failed("codex app-server took too long to list them"))?
        .map_err(|error| listing_error(&error))?;
    let home = home.clone();
    tokio::task::spawn_blocking(move || {
        forget_gone_rollouts();
        let ps_output = processes();
        let mut sessions = to_sessions(&home, threads, &ps_output, SystemTime::now());
        sessions.sort_by_key(|session| Reverse(session.last_used_at));
        sessions
    })
    .await
    .map_err(listing_failed)
}

/// The top-level threads listed as `archived` (or not), most recently updated
/// first, page by page up to [`MAX_THREADS`], or [`MAX_PAGES`] pages, as
/// pages of threads that aren't listed hold none that count.
async fn list_threads(
    transport: &mut impl CodexTransport,
    archived: bool,
) -> Result<Vec<Thread>, CodexRpcError> {
    let mut threads = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
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
            .filter_map(|thread| Thread::read(&thread))
            .filter(|thread| !thread.ephemeral && thread.parent_thread_id.is_none())
            .map(|thread| Thread { archived, ..thread });
        threads.extend(listed);
        cursor = page.next_cursor;
        pages += 1;
        if cursor.is_none() || threads.len() >= MAX_THREADS || pages >= MAX_PAGES {
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
pub(super) fn started_in_desktop(path: &Path) -> bool {
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

/// Forget whether rollouts that are gone were started by the desktop app, so
/// the cache holds only files that are still there.
fn forget_gone_rollouts() {
    DESKTOP_ROLLOUTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .retain(|path, _| path.exists());
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
pub(super) fn lock_path(codex_home: &Path, id: &str) -> PathBuf {
    codex_home
        .join("thread-writer-locks")
        .join(format!("{id}.lock"))
}

/// The error a failed listing shows as.
fn listing_error(error: &CodexRpcError) -> AppError {
    match error {
        CodexRpcError::NotInstalled => AppError::NotInstalled(
            "Install the Codex CLI to see this profile's sessions".to_string(),
        ),
        _ => listing_failed(error),
    }
}

/// The error a listing that failed for `reason` shows as.
fn listing_failed(reason: impl std::fmt::Display) -> AppError {
    AppError::Validation(format!("Couldn't read Codex sessions: {reason}"))
}

#[cfg(test)]
mod tests;
