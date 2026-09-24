//! Moving, archiving and restoring sessions through the API, against two
//! accounts in a temporary home.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ai_profiles_core::api::{
    ArchivedSession, ErrorBody, PairResponse, TransferPlan, TransferReport,
};
use ai_profiles_core::memory::MemoryAction;
use ai_profiles_core::registry::RegistryEntry;
use ai_profiles_core::session_move::ItemAction;
use ai_profiles_core::tls::pinned_client_config;
use ai_profiles_server::certs::Identity;
use ai_profiles_server::config::Config;
use ai_profiles_server::procs::ProcessTable;
use ai_profiles_server::routes::ServerState;
use ai_profiles_server::serve::serve;
use ai_profiles_server::store::Store;
use reqwest::StatusCode;

const SESSION: &str = "22222222-2222-2222-2222-222222222222";

/// Processes that are "live" because a test says so, until signalled.
struct Live(std::sync::Mutex<HashSet<i32>>);

impl ProcessTable for Live {
    fn is_live_claude(&self, entry: &RegistryEntry) -> bool {
        self.0.lock().unwrap().contains(&entry.pid)
    }

    fn signal(&self, entry: &RegistryEntry, _force: bool) -> bool {
        self.0.lock().unwrap().remove(&entry.pid)
    }
}

struct Server {
    base: String,
    token: String,
    fingerprint: String,
    root: PathBuf,
    _home: tempfile::TempDir,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

impl Server {
    fn client(&self) -> reqwest::Client {
        reqwest::Client::builder()
            .use_preconfigured_tls(pinned_client_config(&self.fingerprint).unwrap())
            .build()
            .unwrap()
    }

    fn account(&self, name: &str) -> PathBuf {
        self.root.join(".claude-accounts").join(name)
    }

    async fn get(&self, path: &str) -> reqwest::Response {
        self.client()
            .get(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .send()
            .await
            .unwrap()
    }

    async fn post(&self, path: &str, body: serde_json::Value) -> reqwest::Response {
        self.client()
            .post(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    async fn plan(&self) -> TransferPlan {
        let response = self
            .get(&format!(
                "/v1/accounts/work/sessions/{SESSION}/transfer?to=home"
            ))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        response.json().await.unwrap()
    }
}

/// `work` has the session, with project memory; `home` exists. `live` are
/// the pids that count as running Claude.
async fn start(live: &[i32]) -> Server {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().canonicalize().unwrap();
    let work = root.join(".claude-accounts/work");
    let folder = root.join("code");
    fs::create_dir_all(&folder).unwrap();
    let project = work.join("projects/-code");
    write(
        &project.join(format!("{SESSION}.jsonl")),
        &format!(
            "{}\n{}\n{}\n",
            serde_json::json!({"type":"user","cwd": folder, "slug":"bold-plan"}),
            r#"{"type":"assistant"}"#,
            r#"{"type":"custom-title","customTitle":"Billing"}"#
        ),
    );
    write(&project.join(format!("{SESSION}/subagents/a.jsonl")), "sub");
    write(&work.join(format!("file-history/{SESSION}/f")), "snapshot");
    write(&work.join("plans/bold-plan.md"), "the plan");
    write(&project.join("memory/new.md"), "only in work\n");
    write(&project.join("memory/clash.md"), "work's rule\n");
    write(
        &root.join(".claude-accounts/home/projects/-code/memory/clash.md"),
        "home's rule\n",
    );
    write(&root.join(".claude-accounts/home/.credentials.json"), "{}");

    let mut config = Config::from_toml(r#"listen = "127.0.0.1:0""#, &root).unwrap();
    config.accounts_base = root.join(".claude-accounts");
    let state_dir = root.join("state");
    let identity = Identity::load_or_create(&state_dir).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!(
        "https://127.0.0.1:{}",
        listener.local_addr().unwrap().port()
    );
    let state = Arc::new(ServerState::new(
        config,
        &state_dir,
        Arc::new(Live(std::sync::Mutex::new(live.iter().copied().collect()))),
    ));
    let (shutdown, stop) = tokio::sync::oneshot::channel::<()>();
    let tls = identity.server_config().unwrap();
    tokio::spawn(async move {
        serve(state, listener, tls, async {
            let _ = stop.await;
        })
        .await
        .unwrap();
    });
    let store = Store::new(&state_dir);
    store.add_pending("the-secret", None).unwrap();
    let fingerprint = identity.fingerprint;
    let paired: PairResponse = reqwest::Client::builder()
        .use_preconfigured_tls(pinned_client_config(&fingerprint).unwrap())
        .build()
        .unwrap()
        .post(format!("{base}/v1/pair"))
        .json(&serde_json::json!({"secret": "the-secret", "clientName": "Test Mac"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    Server {
        base,
        token: paired.token,
        fingerprint,
        root,
        _home: home,
        _shutdown: shutdown,
    }
}

async fn code(response: reqwest::Response) -> String {
    response.json::<ErrorBody>().await.unwrap().error.code
}

#[tokio::test]
async fn plans_a_move_and_asks_about_the_memory_both_sides_changed() {
    let server = start(&[]).await;
    let plan = server.plan().await;
    assert_eq!(plan.source, "work");
    assert_eq!(plan.destination, "home");
    assert_eq!(plan.title.as_deref(), Some("Billing"));
    let items: Vec<(String, ItemAction)> = plan
        .items
        .iter()
        .map(|item| (item.path.clone(), item.action))
        .collect();
    assert_eq!(
        items,
        vec![
            (format!("projects/-code/{SESSION}"), ItemAction::Copy),
            (format!("file-history/{SESSION}"), ItemAction::Copy),
            ("plans/bold-plan.md".to_string(), ItemAction::Copy),
            (format!("projects/-code/{SESSION}.jsonl"), ItemAction::Copy),
        ]
    );
    let clash = plan
        .memory
        .iter()
        .find(|file| file.path == "clash.md")
        .unwrap();
    assert_eq!(clash.action, MemoryAction::Conflict);
    assert_eq!(clash.source_text.as_deref(), Some("work's rule\n"));
    assert_eq!(clash.destination_text.as_deref(), Some("home's rule\n"));
    assert!(!plan.destination_newer);
    assert!(plan.running.is_empty());
}

#[tokio::test]
async fn moves_once_every_question_is_answered_then_archives_the_source() {
    let server = start(&[]).await;
    let path = format!("/v1/accounts/work/sessions/{SESSION}/transfer");
    let unanswered = server
        .post(
            &path,
            serde_json::json!({"to": "home", "archiveSource": true}),
        )
        .await;
    assert_eq!(unanswered.status(), StatusCode::CONFLICT);
    assert_eq!(code(unanswered).await, "memory_decision_needed");
    assert!(!server
        .account("home")
        .join(format!("projects/-code/{SESSION}.jsonl"))
        .exists());

    let response = server
        .post(
            &path,
            serde_json::json!({
                "to": "home",
                "archiveSource": true,
                "memory": {"clash.md": {"take": "merged", "text": "both rules\n"}},
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let report: TransferReport = response.json().await.unwrap();
    assert!(report.changed);
    assert!(report
        .memory
        .iter()
        .any(|line| line.contains("added    memory/new.md")));
    assert!(report
        .memory
        .iter()
        .any(|line| line.contains("(by Claude)")));
    let home = server.account("home");
    assert!(home
        .join(format!("projects/-code/{SESSION}.jsonl"))
        .is_file());
    assert!(home
        .join(format!("projects/-code/{SESSION}/subagents/a.jsonl"))
        .is_file());
    assert!(home.join("plans/bold-plan.md").is_file());
    assert_eq!(
        fs::read_to_string(home.join("projects/-code/memory/clash.md")).unwrap(),
        "both rules\n"
    );
    let backup = PathBuf::from(report.backup_dir.expect("home's note was backed up"));
    assert_eq!(
        fs::read_to_string(backup.join("projects/-code/memory/clash.md")).unwrap(),
        "home's rule\n"
    );
    let work = server.account("work");
    assert!(
        !work
            .join(format!("projects/-code/{SESSION}.jsonl"))
            .exists(),
        "archived"
    );
    assert!(
        work.join(format!("file-history/{SESSION}/f")).exists(),
        "only the transcript moves"
    );
    assert!(server
        .root
        .join(".claude-accounts/.claudemulti/memory-base/-code/clash.md")
        .is_file());

    let archived: Vec<ArchivedSession> = server
        .get("/v1/accounts/work/archived")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].title.as_deref(), Some("Billing"));
    let restored = server
        .post(
            &format!(
                "/v1/accounts/work/archived/{SESSION}/{}/restore",
                archived[0].archive
            ),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(restored.status(), StatusCode::OK);
    assert!(work
        .join(format!("projects/-code/{SESSION}.jsonl"))
        .is_file());
}

#[tokio::test]
async fn a_running_session_or_a_newer_destination_copy_needs_saying_so() {
    let server = start(&[4242]).await;
    write(
        &server.account("work").join("sessions/4242.json"),
        &format!(r#"{{"pid":4242,"sessionId":"{SESSION}"}}"#),
    );
    let newer = server
        .account("home")
        .join(format!("projects/-code/{SESSION}.jsonl"));
    write(&newer, "home's later copy\n");
    fs::File::options()
        .write(true)
        .open(&newer)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(120))
        .unwrap();

    let plan = server.plan().await;
    assert!(plan.destination_newer);
    assert_eq!(plan.running.len(), 1);
    assert!(plan.running[0].exact);
    assert_eq!(plan.running[0].account, "work");

    let path = format!("/v1/accounts/work/sessions/{SESSION}/transfer");
    let memory = serde_json::json!({"clash.md": {"take": "destination"}});
    let refused = server
        .post(&path, serde_json::json!({"to": "home", "memory": memory}))
        .await;
    assert_eq!(code(refused).await, "session_running");
    let refused = server
        .post(
            &path,
            serde_json::json!({"to": "home", "memory": memory, "confirmRunning": true}),
        )
        .await;
    assert_eq!(code(refused).await, "destination_newer");
    let moved = server
        .post(
            &path,
            serde_json::json!({"to": "home", "memory": memory, "confirmRunning": true, "replaceNewer": true}),
        )
        .await;
    assert_eq!(moved.status(), StatusCode::OK);
    let report: TransferReport = moved.json().await.unwrap();
    let backup = PathBuf::from(report.backup_dir.unwrap());
    assert_eq!(
        fs::read_to_string(backup.join(format!("projects/-code/{SESSION}.jsonl"))).unwrap(),
        "home's later copy\n"
    );
    assert_eq!(
        fs::read_to_string(
            server
                .account("home")
                .join("projects/-code/memory/clash.md")
        )
        .unwrap(),
        "home's rule\n",
        "kept, as decided"
    );

    let archive = format!("/v1/accounts/work/sessions/{SESSION}/archive");
    let refused = server.post(&archive, serde_json::json!({})).await;
    assert_eq!(code(refused).await, "session_running");
}

#[tokio::test]
async fn archives_a_session_that_is_not_running() {
    let server = start(&[]).await;
    let response = server
        .post(
            &format!("/v1/accounts/work/sessions/{SESSION}/archive"),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    let archived_to = body["archivedTo"].as_str().unwrap();
    assert!(archived_to.contains(&format!("session-transfer-backups/{SESSION}/")));
    assert!(archived_to.contains("-archived/projects/-code/"));
    let listed: Vec<serde_json::Value> = server
        .get("/v1/accounts/work/sessions")
        .await
        .json()
        .await
        .unwrap();
    assert!(listed.iter().all(|session| session["id"] != SESSION));
}

#[tokio::test]
async fn refuses_memory_paths_outside_the_memory_folder() {
    let server = start(&[]).await;
    let response = server
        .post(
            &format!("/v1/accounts/work/sessions/{SESSION}/transfer"),
            serde_json::json!({"to": "home", "memory": {"../../x": {"take": "source"}}}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn exits_a_running_session_and_then_moves_it() {
    let server = start(&[4242]).await;
    write(
        &server.account("work").join("sessions/4242.json"),
        &format!(r#"{{"pid":4242,"sessionId":"{SESSION}"}}"#),
    );
    let moved = server
        .post(
            &format!("/v1/accounts/work/sessions/{SESSION}/transfer"),
            serde_json::json!({
                "to": "home",
                "stopFirst": true,
                "memory": {"clash.md": {"take": "source"}},
                "progressId": "0e0e0e0e-0000-4000-8000-000000000001",
            }),
        )
        .await;
    assert_eq!(
        moved.status(),
        StatusCode::OK,
        "no confirmation needed once it's stopped"
    );
    // Its progress was only kept while it ran.
    assert_eq!(
        server
            .get("/v1/moves/0e0e0e0e-0000-4000-8000-000000000001")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert!(server
        .account("home")
        .join(format!("projects/-code/{SESSION}.jsonl"))
        .is_file());
}

#[tokio::test]
async fn deletes_the_copy_left_behind_when_asked_to_and_says_what_it_freed() {
    let server = start(&[]).await;
    let path = format!("/v1/accounts/work/sessions/{SESSION}/transfer");
    let plan = server.plan().await;
    assert!(
        plan.source_bytes > 0,
        "the dialog can say what deleting frees"
    );
    assert!(
        plan.archive_bytes > 0 && plan.archive_bytes < plan.source_bytes,
        "an archive holds the transcript alone"
    );

    let both = server
        .post(
            &path,
            serde_json::json!({"to": "home", "archiveSource": true, "deleteSource": true}),
        )
        .await;
    assert_eq!(both.status(), StatusCode::BAD_REQUEST);

    let response = server
        .post(
            &path,
            serde_json::json!({
                "to": "home",
                "deleteSource": true,
                "memory": {"clash.md": {"take": "source"}},
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let report: TransferReport = response.json().await.unwrap();
    assert_eq!(report.freed_bytes, Some(plan.source_bytes));
    assert_eq!(report.delete_error, None);
    assert_eq!(report.archived_to, None);

    let work = server.account("work");
    assert!(!work
        .join(format!("projects/-code/{SESSION}.jsonl"))
        .exists());
    assert!(!work.join(format!("projects/-code/{SESSION}")).exists());
    assert!(!work.join(format!("file-history/{SESSION}")).exists());
    assert!(
        work.join("plans/bold-plan.md").is_file(),
        "plans may be shared"
    );
    assert!(
        work.join("projects/-code/memory/new.md").is_file(),
        "memory is the project's"
    );
    assert!(
        !work.join("session-transfer-backups").join(SESSION).exists(),
        "no archive either"
    );
    assert!(server
        .account("home")
        .join(format!("projects/-code/{SESSION}.jsonl"))
        .is_file());
}

#[tokio::test]
async fn deletes_an_archive_for_good_and_says_what_it_freed() {
    let server = start(&[]).await;
    let archived = server
        .post(
            &format!("/v1/accounts/work/sessions/{SESSION}/archive"),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(archived.status(), StatusCode::OK);
    let listed: Vec<serde_json::Value> = server
        .get("/v1/accounts/work/archived")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    let archive = listed[0]["archive"].as_str().unwrap().to_owned();
    let size = listed[0]["sizeBytes"].as_u64().unwrap();
    assert!(size > 0, "the list says what it takes");

    let deleted: serde_json::Value = server
        .client()
        .delete(format!(
            "{}/v1/accounts/work/archived/{SESSION}/{archive}",
            server.base
        ))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(deleted["freedBytes"].as_u64(), Some(size));
    let listed: Vec<serde_json::Value> = server
        .get("/v1/accounts/work/archived")
        .await
        .json()
        .await
        .unwrap();
    assert!(listed.is_empty());
    assert!(!server
        .account("work")
        .join("session-transfer-backups")
        .join(SESSION)
        .exists());

    // Only archives: not anything else under the backups.
    let refused = server
        .client()
        .delete(format!(
            "{}/v1/accounts/work/archived/{SESSION}/..%2F..%2Fprojects",
            server.base
        ))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert!(server.account("work").join("projects").is_dir());
}

#[tokio::test]
async fn takes_a_running_session_out_of_its_account_only_once_it_stopped() {
    let server = start(&[4242]).await;
    write(
        &server.account("work").join("sessions/4242.json"),
        &format!(r#"{{"pid":4242,"sessionId":"{SESSION}"}}"#),
    );
    let path = format!("/v1/accounts/work/sessions/{SESSION}/transfer");
    for afterwards in ["archiveSource", "deleteSource"] {
        let refused = server
            .post(
                &path,
                serde_json::json!({
                    "to": "home",
                    "confirmRunning": true,
                    afterwards: true,
                    "memory": {"clash.md": {"take": "source"}},
                }),
            )
            .await;
        assert_eq!(refused.status(), StatusCode::CONFLICT, "{afterwards}");
        assert_eq!(code(refused).await, "session_running");
    }
    assert!(server
        .account("work")
        .join(format!("projects/-code/{SESSION}.jsonl"))
        .is_file());
    let kept = server
        .post(
            &path,
            serde_json::json!({
                "to": "home",
                "confirmRunning": true,
                "memory": {"clash.md": {"take": "source"}},
            }),
        )
        .await;
    assert_eq!(kept.status(), StatusCode::OK, "a copy, it can have");
}

#[tokio::test]
async fn refuses_to_move_a_session_into_its_own_account_by_another_name() {
    let server = start(&[]).await;
    std::os::unix::fs::symlink(server.account("work"), server.account("alias")).unwrap();
    let refused = server
        .post(
            &format!("/v1/accounts/work/sessions/{SESSION}/transfer"),
            serde_json::json!({"to": "alias", "deleteSource": true}),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert!(server
        .account("work")
        .join(format!("projects/-code/{SESSION}.jsonl"))
        .is_file());
}
