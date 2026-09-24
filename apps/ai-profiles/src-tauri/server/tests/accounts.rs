//! Making, signing in and deleting accounts through the API, with a fake
//! `claude` that acts out `auth login` (prints a link, reads a code) and
//! `auth status`.

use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ai_profiles_core::api::{DeletedAccount, ErrorBody, LoginStart, RemoteAccount};
use ai_profiles_core::registry::RegistryEntry;
use ai_profiles_core::tls::pinned_client_config;
use ai_profiles_server::certs::Identity;
use ai_profiles_server::config::Config;
use ai_profiles_server::procs::ProcessTable;
use ai_profiles_server::routes::ServerState;
use ai_profiles_server::serve::serve;
use ai_profiles_server::store::Store;
use reqwest::StatusCode;

/// Signs in only with `good#code`, writing what a real sign-in writes.
fn fake_claude(sign_in_link: &str) -> String {
    format!(
        r#"#!/bin/bash
cfg="$CLAUDE_CONFIG_DIR"
if [ "$1 $2" = "auth login" ]; then
  echo "Opening browser to sign in…"
  echo "If the browser didn't open, visit: {sign_in_link}"
  printf "Paste code here if prompted > "
  read -r code
  if [ "$code" = "good#code" ]; then
    echo '{{"oauthAccount":{{"emailAddress":"new@example.com","organizationType":"claude_pro"}}}}' > "$cfg/.claude.json"
    echo '{{}}' > "$cfg/.credentials.json"
    echo "Login successful."
    exit 0
  fi
  echo "Login failed: Request failed with status code 400"
  exit 1
fi
if [ "$1 $2" = "auth status" ]; then
  if [ -f "$cfg/.credentials.json" ]; then echo '{{"loggedIn":true}}'; else echo '{{"loggedIn":false}}'; fi
  exit 0
fi
exit 2
"#
    )
}

struct Live(HashSet<i32>);

impl ProcessTable for Live {
    fn is_live_claude(&self, entry: &RegistryEntry) -> bool {
        self.0.contains(&entry.pid)
    }

    // Nothing here stops a session.
    fn signal(&self, _entry: &RegistryEntry, _force: bool) -> bool {
        false
    }
}

struct Server {
    base: String,
    token: String,
    fingerprint: String,
    root: PathBuf,
    _home: tempfile::TempDir,
    _stop: tokio::sync::oneshot::Sender<()>,
}

impl Server {
    fn client(&self) -> reqwest::Client {
        reqwest::Client::builder()
            .use_preconfigured_tls(pinned_client_config(&self.fingerprint).unwrap())
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap()
    }

    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> reqwest::Response {
        let mut request = self
            .client()
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        request.send().await.unwrap()
    }

    async fn post(&self, path: &str, body: serde_json::Value) -> reqwest::Response {
        self.send(reqwest::Method::POST, path, Some(body)).await
    }

    async fn delete(&self, path: &str) -> reqwest::Response {
        self.send(reqwest::Method::DELETE, path, None).await
    }

    fn base_dir(&self) -> PathBuf {
        self.root.join(".claude-accounts")
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

async fn start_with(sign_in_link: &str, live: HashSet<i32>) -> Server {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().canonicalize().unwrap();
    let claude = root.join("bin/claude");
    write(&claude, &fake_claude(sign_in_link));
    fs::set_permissions(&claude, fs::Permissions::from_mode(0o755)).unwrap();
    let mut config = Config::from_toml(
        &format!(
            "listen = \"127.0.0.1:0\"\nclaude_path = \"{}\"",
            claude.display()
        ),
        &root,
    )
    .unwrap();
    config.accounts_base = root.join(".claude-accounts");
    let state_dir = root.join("state");
    let identity = Identity::load_or_create(&state_dir).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!(
        "https://127.0.0.1:{}",
        listener.local_addr().unwrap().port()
    );
    let state = Arc::new(ServerState::new(config, &state_dir, Arc::new(Live(live))));
    let tls = identity.server_config().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = serve(state, listener, tls, async {
            let _ = stopped.await;
        })
        .await;
    });
    let store = Store::new(&state_dir);
    store.add_pending("secret", None).unwrap();
    let (_, token) = store.redeem("secret", "test").unwrap().unwrap();
    Server {
        base,
        token,
        fingerprint: identity.fingerprint,
        root,
        _home: home,
        _stop: stop,
    }
}

async fn start() -> Server {
    start_with(
        "https://claude.com/cai/oauth/authorize?code=true&state=abc",
        HashSet::new(),
    )
    .await
}

async fn code_of(response: reqwest::Response) -> String {
    response.json::<ErrorBody>().await.unwrap().error.code
}

#[tokio::test(flavor = "multi_thread")]
async fn creates_an_account_once_with_a_plain_name() {
    let server = start().await;
    let made = server
        .post("/v1/accounts", serde_json::json!({"name": "work"}))
        .await;
    assert_eq!(made.status(), StatusCode::CREATED);
    let account: RemoteAccount = made.json().await.unwrap();
    assert_eq!(account.name, "work");
    assert!(!account.signed_in);
    assert!(server.base_dir().join("work").is_dir());

    let again = server
        .post("/v1/accounts", serde_json::json!({"name": "work"}))
        .await;
    assert_eq!(again.status(), StatusCode::CONFLICT);
    assert_eq!(code_of(again).await, "name_taken");
    for bad in ["../escape", "default", ".hidden", "has space", ""] {
        let refused = server
            .post("/v1/accounts", serde_json::json!({"name": bad}))
            .await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST, "{bad}");
    }
    assert!(!server.root.join("escape").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn signs_an_account_in_with_the_code_from_the_sign_in_page() {
    let server = start().await;
    server
        .post("/v1/accounts", serde_json::json!({"name": "work"}))
        .await;

    // A wrong code ends that sign-in.
    let first: LoginStart = server
        .post("/v1/accounts/work/login", serde_json::json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        first.url,
        "https://claude.com/cai/oauth/authorize?code=true&state=abc"
    );
    let wrong = server
        .post(
            &format!("/v1/logins/{}/code", first.login_id),
            serde_json::json!({"code": "bad#code"}),
        )
        .await;
    assert_eq!(wrong.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(code_of(wrong).await, "login_failed");
    let reused = server
        .post(
            &format!("/v1/logins/{}/code", first.login_id),
            serde_json::json!({"code": "good#code"}),
        )
        .await;
    assert_eq!(reused.status(), StatusCode::GONE);

    // A fresh one with the right code signs it in.
    let second: LoginStart = server
        .post("/v1/accounts/work/login", serde_json::json!({}))
        .await
        .json()
        .await
        .unwrap();
    let signed: RemoteAccount = server
        .post(
            &format!("/v1/logins/{}/code", second.login_id),
            serde_json::json!({"code": "good#code"}),
        )
        .await
        .json()
        .await
        .unwrap();
    assert!(signed.signed_in);
    assert_eq!(
        signed.account.unwrap().email.as_deref(),
        Some("new@example.com")
    );
    // Set up too, so its first session doesn't stop at Claude's first-run setup.
    let written: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(server.base_dir().join("work/.claude.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(written["hasCompletedOnboarding"], true);
    assert_eq!(written["oauthAccount"]["emailAddress"], "new@example.com");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_sign_in_is_over_and_nonsense_codes_are_refused() {
    let server = start().await;
    server
        .post("/v1/accounts", serde_json::json!({"name": "work"}))
        .await;
    let login: LoginStart = server
        .post("/v1/accounts/work/login", serde_json::json!({}))
        .await
        .json()
        .await
        .unwrap();
    let nonsense = server
        .post(
            &format!("/v1/logins/{}/code", login.login_id),
            serde_json::json!({"code": "has spaces\nand lines"}),
        )
        .await;
    assert_eq!(nonsense.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        server
            .delete(&format!("/v1/logins/{}", login.login_id))
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    let after = server
        .post(
            &format!("/v1/logins/{}/code", login.login_id),
            serde_json::json!({"code": "good#code"}),
        )
        .await;
    assert_eq!(after.status(), StatusCode::GONE);
    assert!(!server.base_dir().join("work/.credentials.json").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn never_passes_on_a_link_to_somewhere_else() {
    let server = start_with("https://claude.com.evil.example/steal", HashSet::new()).await;
    server
        .post("/v1/accounts", serde_json::json!({"name": "work"}))
        .await;
    let refused = server
        .post("/v1/accounts/work/login", serde_json::json!({}))
        .await;
    assert_eq!(refused.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = refused.json::<ErrorBody>().await.unwrap();
    assert_eq!(body.error.code, "login_failed");
    assert!(!body.error.message.contains("evil"));
}

#[tokio::test(flavor = "multi_thread")]
async fn deletes_to_the_trash_but_not_while_something_uses_the_account() {
    let server = start_with("https://claude.com/x", HashSet::from([4242])).await;
    server
        .post("/v1/accounts", serde_json::json!({"name": "busy"}))
        .await;
    server
        .post("/v1/accounts", serde_json::json!({"name": "idle"}))
        .await;
    write(
        &server.base_dir().join("busy/sessions/4242.json"),
        r#"{"pid":4242,"sessionId":"11111111-1111-1111-1111-111111111111"}"#,
    );
    let busy = server.delete("/v1/accounts/busy").await;
    assert_eq!(busy.status(), StatusCode::CONFLICT);
    assert_eq!(code_of(busy).await, "account_busy");

    let login: LoginStart = server
        .post("/v1/accounts/idle/login", serde_json::json!({}))
        .await
        .json()
        .await
        .unwrap();
    let signing_in = server.delete("/v1/accounts/idle").await;
    assert_eq!(signing_in.status(), StatusCode::CONFLICT);
    server
        .delete(&format!("/v1/logins/{}", login.login_id))
        .await;

    let deleted: DeletedAccount = server
        .delete("/v1/accounts/idle")
        .await
        .json()
        .await
        .unwrap();
    assert!(deleted.trashed_to.contains("/.trash/idle-"));
    assert!(Path::new(&deleted.trashed_to).is_dir());
    assert!(!server.base_dir().join("idle").exists());
    let listed: Vec<RemoteAccount> = server
        .send(reqwest::Method::GET, "/v1/accounts", None)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        listed.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        vec!["busy"]
    );
    assert_eq!(
        server.delete("/v1/accounts/nobody").await.status(),
        StatusCode::NOT_FOUND
    );
}
