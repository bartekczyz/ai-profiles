use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tempfile::tempdir;

use super::*;
use crate::sessions::codex::fakes::{home, running_desktop, write_lock, write_rollout};

const PAGE: &str = include_str!("../../fixtures/codex-thread-list.json");

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
            ..Thread::read(&thread).unwrap()
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
    let mut server = FakeServer::new(vec![json!({ "data": [], "nextCursor": "same" }); 3], vec![]);

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
async fn a_thread_is_read_field_by_field_keeping_what_parses() {
    let odd = json!({
        "id": "odd",
        "name": 7,
        "preview": "Hi",
        "cwd": "/work/app",
        "updatedAt": "last tuesday",
        "recencyAt": 1789000000,
        "ephemeral": "no",
        "status": "active",
    });
    let textual = json!({ "id": "textual", "updatedAt": "1789000001" });
    let mut server = FakeServer::new(
        vec![json!({ "data": [odd, textual, { "name": "no id" }], "nextCursor": null })],
        vec![],
    );

    let threads = list_threads(&mut server, false).await.unwrap();

    let read: Vec<Value> = threads
        .iter()
        .map(|thread| {
            json!({
                "id": thread.id,
                "name": thread.name,
                "cwd": thread.cwd,
                "updatedAt": thread.updated_at,
                "status": thread.status.as_ref().map(|status| &status.kind),
            })
        })
        .collect();
    assert_eq!(
        read,
        [
            json!({ "id": "odd", "name": null, "cwd": "/work/app", "updatedAt": 1789000000, "status": "active" }),
            json!({ "id": "textual", "name": null, "cwd": null, "updatedAt": 1789000001, "status": null }),
        ]
    );
}

#[tokio::test]
async fn a_thread_without_an_update_time_goes_by_its_recency_then_its_creation() {
    let mut server = FakeServer::new(
        vec![json!({ "data": [
            { "id": "recent", "recencyAt": 1789000002, "createdAt": 1789000000 },
            { "id": "created", "createdAt": 1789000003 },
            { "id": "undated" },
        ], "nextCursor": null })],
        vec![],
    );

    let threads = list_threads(&mut server, false).await.unwrap();

    let dated: Vec<(&str, i64)> = threads
        .iter()
        .map(|thread| (thread.id.as_str(), thread.updated_at))
        .collect();
    assert_eq!(
        dated,
        [
            ("recent", 1789000002),
            ("created", 1789000003),
            ("undated", 0)
        ]
    );
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
    let server = FakeServer::new(
        vec![json!({ "data": [thread("live")], "nextCursor": null })],
        vec![json!({ "data": [shelved], "nextCursor": null })],
    );

    let sessions = list_with(
        async { Ok(server) },
        &home(root.path(), false),
        LISTING_TIMEOUT,
        String::new,
    )
    .await
    .unwrap();

    let listed: Vec<(&str, bool)> = sessions
        .iter()
        .map(|session| (session.id.as_str(), session.archived))
        .collect();
    assert_eq!(listed, [("shelved", true), ("live", false)]);
}

/// A transport that says when it is dropped, answering through `inner`.
struct Watched<T> {
    /// Answers the calls.
    inner: T,
    /// Set once this is dropped.
    dropped: Arc<AtomicBool>,
}

impl<T> Drop for Watched<T> {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

#[async_trait]
impl<T: CodexTransport> CodexTransport for Watched<T> {
    async fn request(&mut self, method: &str, params: Value) -> Result<Value, CodexRpcError> {
        self.inner.request(method, params).await
    }
}

/// A transport that never answers.
struct Hanging;

#[async_trait]
impl CodexTransport for Hanging {
    async fn request(&mut self, _: &str, _: Value) -> Result<Value, CodexRpcError> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn the_app_server_is_let_go_before_the_rows_are_worked_out() {
    let root = tempdir().unwrap();
    let dropped = Arc::new(AtomicBool::new(false));
    let server = Watched {
        inner: FakeServer::new(
            vec![json!({ "data": [thread("live")], "nextCursor": null })],
            vec![],
        ),
        dropped: dropped.clone(),
    };
    let seen = Arc::new(AtomicBool::new(false));
    let seen_by_processes = seen.clone();
    let processes = move || {
        seen_by_processes.store(dropped.load(Ordering::SeqCst), Ordering::SeqCst);
        String::new()
    };

    list_with(
        async { Ok(server) },
        &home(root.path(), false),
        LISTING_TIMEOUT,
        processes,
    )
    .await
    .unwrap();

    assert!(seen.load(Ordering::SeqCst));
}

#[tokio::test]
async fn a_listing_that_takes_too_long_fails_with_a_clear_error() {
    let root = tempdir().unwrap();

    let listed = list_with(
        async { Ok(Hanging) },
        &home(root.path(), false),
        Duration::from_millis(20),
        String::new,
    )
    .await;

    assert!(matches!(
        listed,
        Err(AppError::Validation(message))
            if message == "Couldn't read Codex sessions: codex app-server took too long to list them"
    ));
}

#[tokio::test]
async fn a_listing_whose_pages_keep_coming_empty_of_threads_stops() {
    let subagents = json!({
        "data": [{ "id": "sub", "parentThreadId": "parent", "updatedAt": 1 }],
        "nextCursor": "more",
    });
    let mut server = FakeServer::new(vec![subagents; 30], vec![]);

    let threads = list_threads(&mut server, false).await.unwrap();

    assert!(threads.is_empty());
    assert_eq!(server.requests.len(), MAX_THREADS / PAGE_SIZE + 2);
}

#[tokio::test]
async fn a_failure_working_out_the_rows_reads_as_a_listing_failure() {
    let root = tempdir().unwrap();
    let server = FakeServer::new(vec![], vec![]);

    let listed = list_with(
        async { Ok(server) },
        &home(root.path(), false),
        LISTING_TIMEOUT,
        || panic!("no process list"),
    )
    .await;

    assert!(matches!(
        listed,
        Err(AppError::Validation(message)) if message.starts_with("Couldn't read Codex sessions: ")
    ));
}

#[tokio::test]
async fn a_listing_forgets_rollouts_that_are_gone() {
    let root = tempdir().unwrap();
    let gone = write_rollout(root.path(), "gone", DESKTOP_ORIGINATOR);
    let kept = write_rollout(root.path(), "kept", DESKTOP_ORIGINATOR);
    assert!(started_in_desktop(&gone));
    assert!(started_in_desktop(&kept));
    fs::remove_file(&gone).unwrap();

    list_with(
        async { Ok(FakeServer::new(vec![], vec![])) },
        &home(root.path(), false),
        LISTING_TIMEOUT,
        String::new,
    )
    .await
    .unwrap();

    let cache = DESKTOP_ROLLOUTS.lock().unwrap();
    assert!(!cache.contains_key(&gone));
    assert!(cache.contains_key(&kept));
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
                unmovable_reason: Some("Codex has it open — close it first".to_string()),
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
