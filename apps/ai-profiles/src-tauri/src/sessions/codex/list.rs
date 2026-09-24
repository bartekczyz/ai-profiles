//! Listing a `CODEX_HOME`'s threads through `codex app-server`'s `thread/list`.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::fs::{self, File};
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

/// The fields of a `thread/list` thread read here.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    #[serde(default)]
    updated_at: i64,
    /// The thread is never written to disk.
    #[serde(default)]
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
    #[serde(skip)]
    archived: bool,
}

/// A thread's `status`.
#[derive(Debug, Deserialize)]
pub(super) struct ThreadStatus {
    /// `notLoaded`, `idle`, `systemError` or `active`.
    #[serde(rename = "type")]
    pub(super) kind: String,
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
        _ => AppError::Validation(format!("Couldn't read Codex sessions: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use tempfile::tempdir;

    use super::*;
    use crate::sessions::codex::fakes::{home, running_desktop, write_lock, write_rollout};

    const PAGE: &str = include_str!("../fixtures/codex-thread-list.json");

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
        /// A server answering the active listing with the pages `active`, then
        /// the archived one with `archived`.
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

    /// The recorded `thread/list` page the tests read.
    fn page() -> Value {
        serde_json::from_str(PAGE).unwrap()
    }

    /// A listed thread `id`, with the fields a row needs.
    fn thread(id: &str) -> Value {
        json!({ "id": id, "preview": "Hi", "cwd": "/work/app", "updatedAt": 1789000000 })
    }

    /// The time `seconds` after the epoch.
    fn utc(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    /// The threads of `value`, a `thread/list` page, each `archived` or not.
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

        assert!(matches!(
            missing,
            AppError::NotInstalled(message)
                if message == "Install the Codex CLI to see this profile's sessions"
        ));
        assert!(matches!(
            failed,
            AppError::Validation(message)
                if message == "Couldn't read Codex sessions: codex app-server exited before answering"
        ));
    }
}
