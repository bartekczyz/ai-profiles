//! The server end to end: a real listener on 127.0.0.1, real TLS, and a
//! client that pins the certificate the way ai-profiles does.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use ai_profiles_core::api::{
    DirListing, ErrorBody, HostInfo, PairResponse, Ping, RemoteAccount, RemoteSession,
};
use ai_profiles_core::registry::RegistryEntry;
use ai_profiles_core::tls::pinned_client_config;
use ai_profiles_server::certs::Identity;
use ai_profiles_server::config::Config;
use ai_profiles_server::procs::ProcessTable;
use ai_profiles_server::routes::ServerState;
use ai_profiles_server::serve::serve;
use ai_profiles_server::store::Store;
use reqwest::StatusCode;

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

const SESSION: &str = "11111111-1111-1111-1111-111111111111";

struct Server {
    base: String,
    fingerprint: String,
    store: Store,
    home: tempfile::TempDir,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl Server {
    fn client(&self) -> reqwest::Client {
        client_pinned_to(&self.fingerprint)
    }

    /// Pair a client the way the Mac does, returning its token.
    async fn pair(&self) -> String {
        self.store.add_pending("the-secret", None).unwrap();
        let response: PairResponse = self
            .client()
            .post(format!("{}/v1/pair", self.base))
            .json(&serde_json::json!({"secret": "the-secret", "clientName": "Test Mac"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        response.token
    }
}

fn client_pinned_to(fingerprint: &str) -> reqwest::Client {
    reqwest::Client::builder()
        .use_preconfigured_tls(pinned_client_config(fingerprint).unwrap())
        .build()
        .unwrap()
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

async fn start() -> Server {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().canonicalize().unwrap();
    let account = root.join(".claude-accounts/work");
    write(
        &account.join(".claude.json"),
        r#"{"oauthAccount":{"emailAddress":"ada@example.com","organizationType":"claude_max"}}"#,
    );
    write(&account.join(".credentials.json"), "{}");
    write(
        &account.join(format!("projects/-code/{SESSION}.jsonl")),
        concat!(
            r#"{"type":"user","cwd":"/code"}"#,
            "\n",
            r#"{"type":"assistant"}"#,
            "\n",
            r#"{"type":"custom-title","customTitle":"Billing"}"#,
            "\n"
        ),
    );
    write(
        &account.join("sessions/77.json"),
        &format!(r#"{{"pid":77,"sessionId":"{SESSION}","tmux":"ai:@2.%4"}}"#),
    );
    fs::create_dir_all(root.join("code/api")).unwrap();

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
        Arc::new(Live(HashSet::from([77]))),
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
    Server {
        base,
        fingerprint: identity.fingerprint,
        store: Store::new(&state_dir),
        home,
        _shutdown: shutdown,
    }
}

async fn error_code(response: reqwest::Response) -> String {
    response.json::<ErrorBody>().await.unwrap().error.code
}

#[tokio::test]
async fn ping_needs_no_token_and_says_the_api_version() {
    let server = start().await;
    let response = server
        .client()
        .get(format!("{}/v1/ping", server.base))
        .send()
        .await
        .unwrap();
    assert_eq!(response.headers()["x-aip-api"], "1");
    assert_eq!(response.json::<Ping>().await.unwrap().api_version, 1);
}

#[tokio::test]
async fn a_client_pinned_to_another_certificate_cannot_connect() {
    let server = start().await;
    let wrong = ai_profiles_core::pairing::fingerprint(b"another certificate");
    let result = client_pinned_to(&wrong)
        .get(format!("{}/v1/ping", server.base))
        .send()
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn pairing_gives_a_token_once_and_the_token_opens_the_api() {
    let server = start().await;
    let client = server.client();
    let without = client
        .get(format!("{}/v1/accounts", server.base))
        .send()
        .await
        .unwrap();
    assert_eq!(without.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(without).await, "unauthorized");

    let token = server.pair().await;
    let again = client
        .post(format!("{}/v1/pair", server.base))
        .json(&serde_json::json!({"secret": "the-secret", "clientName": "x"}))
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(again).await, "pairing_invalid");

    let info: HostInfo = client
        .get(format!("{}/v1/info", server.base))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(info.api_version, 1);
    assert_eq!(
        info.tmux.map(|tmux| tmux.session).unwrap_or("ai".into()),
        "ai"
    );
}

#[tokio::test]
async fn lists_accounts_and_their_sessions() {
    let server = start().await;
    let token = server.pair().await;
    let client = server.client();

    let accounts: Vec<RemoteAccount> = client
        .get(format!("{}/v1/accounts", server.base))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(accounts.len(), 1);
    let work = &accounts[0];
    assert_eq!(work.name, "work");
    assert!(work.signed_in);
    assert_eq!(
        work.account.as_ref().unwrap().email.as_deref(),
        Some("ada@example.com")
    );
    assert_eq!((work.sessions, work.running_sessions), (1, 1));

    let sessions: Vec<RemoteSession> = client
        .get(format!("{}/v1/accounts/work/sessions", server.base))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].title.as_deref(), Some("Billing"));
    assert_eq!(sessions[0].window.as_ref().unwrap().window_id, "@2");

    let missing = client
        .get(format!("{}/v1/accounts/nobody/sessions", server.base))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn browses_folders_but_not_past_home() {
    let server = start().await;
    let token = server.pair().await;
    let client = server.client();
    let listing: DirListing = client
        .get(format!("{}/v1/fs/dirs", server.base))
        .query(&[("path", "~/code")])
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(listing.entries[0].name, "api");

    let outside = client
        .get(format!("{}/v1/fs/dirs", server.base))
        .query(&[("path", "/")])
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(outside.status(), StatusCode::FORBIDDEN);
    assert_eq!(error_code(outside).await, "forbidden_path");
    assert!(server.home.path().exists());
}

#[tokio::test]
async fn a_revoked_or_unpaired_client_is_shut_out() {
    let server = start().await;
    let client = server.client();
    let token = server.pair().await;
    let removed = client
        .delete(format!("{}/v1/clients/self", server.base))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    let after = client
        .get(format!("{}/v1/info", server.base))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(after.status(), StatusCode::UNAUTHORIZED);

    let second = server.pair().await;
    server.store.revoke("Test Mac").unwrap();
    let revoked = client
        .get(format!("{}/v1/info", server.base))
        .bearer_auth(&second)
        .send()
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refuses_web_pages_and_locks_out_token_guessing() {
    let server = start().await;
    let client = server.client();
    let token = server.pair().await;
    let from_page = client
        .get(format!("{}/v1/info", server.base))
        .bearer_auth(&token)
        .header("Origin", "https://evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(from_page.status(), StatusCode::FORBIDDEN);

    for _ in 0..11 {
        client
            .get(format!("{}/v1/info", server.base))
            .bearer_auth("aip_guess")
            .send()
            .await
            .unwrap();
    }
    let locked = client
        .get(format!("{}/v1/info", server.base))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(locked.status(), StatusCode::TOO_MANY_REQUESTS);
}
