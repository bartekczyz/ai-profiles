//! The client against a real ai-profiles-server, in-process on 127.0.0.1.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use ai_profiles_core::api::WindowKey;
use ai_profiles_core::pairing::{encode, PairingCode};
use ai_profiles_core::registry::RegistryEntry;
use ai_profiles_server::certs::Identity;
use ai_profiles_server::config::Config;
use ai_profiles_server::procs::ProcessTable;
use ai_profiles_server::routes::ServerState;
use ai_profiles_server::serve::serve;
use ai_profiles_server::store::Store;

use super::hosts::HostList;
use super::secrets::{MemoryStore, SecretStore};
use super::*;

struct NothingRunning;

impl ProcessTable for NothingRunning {
    fn is_live_claude(&self, _entry: &RegistryEntry) -> bool {
        false
    }

    fn signal(&self, _entry: &RegistryEntry, _force: bool) -> bool {
        false
    }
}

struct Server {
    address: String,
    fingerprint: String,
    store: Store,
    _home: tempfile::TempDir,
    _stop: tokio::sync::oneshot::Sender<()>,
}

impl Server {
    /// A fresh pairing code for this server.
    fn code(&self, secret: &str) -> String {
        self.store.add_pending(secret, None).unwrap();
        encode(&PairingCode {
            v: 1,
            hosts: vec![self.address.clone()],
            secret: secret.into(),
            fp: self.fingerprint.clone(),
        })
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

async fn start_server() -> Server {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().canonicalize().unwrap();
    let account = root.join(".claude-accounts/work");
    write(
        &account.join(".claude.json"),
        r#"{"oauthAccount":{"emailAddress":"ada@example.com","organizationType":"claude_pro"}}"#,
    );
    write(
        &account.join("projects/-code/11111111-1111-1111-1111-111111111111.jsonl"),
        "{\"type\":\"user\",\"cwd\":\"/code\"}\n{\"type\":\"assistant\"}\n{\"type\":\"ai-title\",\"aiTitle\":\"Refactor\"}\n",
    );
    let mut config = Config::from_toml("listen = \"127.0.0.1:0\"", &root).unwrap();
    config.accounts_base = root.join(".claude-accounts");
    let state_dir = root.join("state");
    let identity = Identity::load_or_create(&state_dir).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
    let state = Arc::new(ServerState::new(
        config,
        &state_dir,
        Arc::new(NothingRunning),
    ));
    let tls = identity.server_config().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = serve(state, listener, tls, async {
            let _ = stopped.await;
        })
        .await;
    });
    Server {
        address,
        fingerprint: identity.fingerprint,
        store: Store::new(&state_dir),
        _home: home,
        _stop: stop,
    }
}

fn code_of(error: AppError) -> String {
    match error {
        AppError::Remote { code, .. } => code,
        other => panic!("expected a remote error, got {other:?}"),
    }
}

fn list_in(dir: &tempfile::TempDir) -> HostList {
    HostList::at(dir.path().join("remote-hosts.json"))
}

#[tokio::test]
async fn pairs_then_lists_accounts_and_sessions() {
    let server = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let list = list_in(&dir);
    let secrets = MemoryStore::default();

    let host = pair(&list, &secrets, &server.code("s1"), None)
        .await
        .unwrap();
    assert_eq!(
        host.last_good_address.as_deref(),
        Some(server.address.as_str())
    );
    assert!(!host.client_id.is_empty());
    assert!(secrets.get(&host.id).unwrap().unwrap().starts_with("aip_"));
    assert_eq!(list.load().unwrap(), vec![host.clone()]);

    let accounts = accounts(&list, &secrets, &host.id).await.unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].name, "work");
    assert_eq!(
        accounts[0].account.as_ref().unwrap().plan.as_deref(),
        Some("Pro")
    );

    let sessions = sessions(&list, &secrets, &host.id, "work").await.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].title.as_deref(), Some("Refactor"));

    let missing = super::sessions(&list, &secrets, &host.id, "nobody")
        .await
        .unwrap_err();
    assert_eq!(code_of(missing), "not_found");
    let odd = super::sessions(&list, &secrets, &host.id, "../etc")
        .await
        .unwrap_err();
    assert!(matches!(odd, AppError::Validation(_)));
}

#[tokio::test]
async fn pairing_the_same_server_again_refreshes_rather_than_duplicates() {
    let server = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let list = list_in(&dir);
    let secrets = MemoryStore::default();
    let first = pair(&list, &secrets, &server.code("a"), Some("Linux box".into()))
        .await
        .unwrap();
    let second = pair(&list, &secrets, &server.code("b"), None)
        .await
        .unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(second.label, "Linux box", "keeps the name it was given");
    assert_eq!(list.load().unwrap().len(), 1);
    // A used code stays used. (Three attempts: a fourth within the minute
    // would meet the server's pairing rate limit instead.)
    let reused = encode(&PairingCode {
        v: 1,
        hosts: vec![server.address.clone()],
        secret: "b".into(),
        fp: server.fingerprint.clone(),
    });
    let error = pair(&list, &secrets, &reused, None).await.unwrap_err();
    assert_eq!(code_of(error), "pairing_invalid");
}

#[tokio::test]
async fn refuses_a_server_with_another_certificate() {
    let server = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let list = list_in(&dir);
    let secrets = MemoryStore::default();
    server.store.add_pending("s", None).unwrap();
    let impostor = encode(&PairingCode {
        v: 1,
        hosts: vec![server.address.clone()],
        secret: "s".into(),
        fp: ai_profiles_core::pairing::fingerprint(b"a different certificate"),
    });
    let error = pair(&list, &secrets, &impostor, None).await.unwrap_err();
    assert_eq!(code_of(error), "cert_mismatch");
    assert!(list.load().unwrap().is_empty());
}

#[tokio::test]
async fn says_offline_when_no_address_answers_and_uses_the_next_that_does() {
    let server = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let list = list_in(&dir);
    let secrets = MemoryStore::default();
    let host = pair(&list, &secrets, &server.code("s"), None)
        .await
        .unwrap();

    // A dead address first: the live one is still found.
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = format!("127.0.0.1:{}", closed.local_addr().unwrap().port());
    drop(closed);
    list.update(&host.id, |host| {
        host.addresses = vec![dead.clone(), server.address.clone()];
        host.last_good_address = None;
    })
    .unwrap();
    assert!(accounts(&list, &secrets, &host.id).await.is_ok());
    assert_eq!(
        list.find(&host.id).unwrap().last_good_address.as_deref(),
        Some(server.address.as_str())
    );

    list.update(&host.id, |host| {
        host.addresses = vec![dead.clone()];
        host.last_good_address = None;
    })
    .unwrap();
    let error = accounts(&list, &secrets, &host.id).await.unwrap_err();
    assert_eq!(code_of(error), "offline");
}

/// Takes each request and never answers it.
async fn silent_host(identity: &Identity) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
    let acceptor = tokio_rustls::TlsAcceptor::from(identity.server_config().unwrap());
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncReadExt;
                if let Ok(mut tls) = acceptor.accept(stream).await {
                    let mut buffer = [0u8; 4096];
                    while tls.read(&mut buffer).await.is_ok_and(|read| read > 0) {}
                }
            });
        }
    });
    address
}

/// Counts the connections made to it.
async fn counting_host() -> (String, Arc<std::sync::atomic::AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = count.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            drop(stream);
        }
    });
    (address, count)
}

#[tokio::test]
async fn never_sends_a_change_again_to_another_address_after_the_host_took_it() {
    let state = tempfile::tempdir().unwrap();
    let identity = Identity::load_or_create(state.path()).unwrap();
    let silent = silent_host(&identity).await;
    let (other, connections) = counting_host().await;
    let host = RemoteHost {
        id: "h".into(),
        label: "xjopa1".into(),
        hostname: "xjopa1".into(),
        addresses: vec![silent, other],
        fingerprint: identity.fingerprint.clone(),
        client_id: "c".into(),
        paired_at: "2026-09-22T00:00:00Z".into(),
        last_good_address: None,
        profiles: Default::default(),
    };
    let client = client::HostClient::new(host, Some("aip_x".into()))
        .unwrap()
        .patient(std::time::Duration::from_millis(500));

    // A move the host took but hasn't answered may still be going: sending
    // it to the next address would move the session twice.
    let error = client
        .post::<_, serde_json::Value>("/v1/accounts/a/sessions/x/transfer", &serde_json::json!({}))
        .await
        .map(|_| ())
        .unwrap_err();
    assert_eq!(code_of(error), "timeout");
    assert_eq!(connections.load(std::sync::atomic::Ordering::SeqCst), 0);

    // Reading is safe to try elsewhere.
    let _ = client
        .get::<serde_json::Value>("/v1/info", &[])
        .await
        .map(|_| ());
    assert_eq!(connections.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_revoked_client_is_told_so_and_removing_a_host_unpairs_it() {
    let server = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let list = list_in(&dir);
    let secrets = MemoryStore::default();
    let host = pair(&list, &secrets, &server.code("s"), None)
        .await
        .unwrap();
    assert_eq!(server.store.clients().unwrap().len(), 1);

    remove(&list, &secrets, &host.id).await.unwrap();
    assert!(list.load().unwrap().is_empty());
    assert_eq!(secrets.get(&host.id).unwrap(), None);
    assert!(
        server.store.clients().unwrap().is_empty(),
        "the server forgot us too"
    );

    let again = pair(&list, &secrets, &server.code("t"), None)
        .await
        .unwrap();
    server.store.revoke(&again.client_id).unwrap();
    let error = info(&list, &secrets, &again.id).await.unwrap_err();
    assert_eq!(code_of(error), "unauthorized");
}

#[tokio::test]
async fn browses_the_hosts_folders_and_checks_session_ids_before_sending() {
    let server = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let list = list_in(&dir);
    let secrets = MemoryStore::default();
    let host = pair(&list, &secrets, &server.code("s"), None)
        .await
        .unwrap();

    let home = dirs(&list, &secrets, &host.id, None).await.unwrap();
    assert!(home.entries.iter().any(|entry| entry.name == "state"));
    let outside = dirs(&list, &secrets, &host.id, Some("/"))
        .await
        .unwrap_err();
    assert_eq!(code_of(outside), "forbidden_path");

    let bad = resume(&list, &secrets, &host.id, "work", "../../x", true)
        .await
        .unwrap_err();
    assert!(matches!(bad, AppError::Validation(_)));
    let unknown = resume(
        &list,
        &secrets,
        &host.id,
        "work",
        "00000000-0000-0000-0000-000000000000",
        true,
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(unknown), "not_found");
}

#[tokio::test]
async fn checks_window_ids_before_sending_and_hears_when_a_window_is_gone() {
    let server = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let list = list_in(&dir);
    let secrets = MemoryStore::default();
    let host = pair(&list, &secrets, &server.code("s"), None)
        .await
        .unwrap();

    for bad in ["", "@", "1", "@1/../x", "@1;kill-server"] {
        let refused = window_screen(&list, &secrets, &host.id, "work", bad)
            .await
            .unwrap_err();
        assert!(matches!(refused, AppError::Validation(_)), "{bad}");
    }
    let gone = window_keys(
        &list,
        &secrets,
        &host.id,
        "work",
        "@99999",
        vec![WindowKey::Key("Enter".into())],
    )
    .await
    .unwrap_err();
    assert_eq!(code_of(gone), "window_gone");
}

#[test]
fn opens_only_a_plain_tmux_attach_in_terminal() {
    let host = RemoteHost {
        id: "h".into(),
        label: "xjopa1".into(),
        hostname: "xjopa1".into(),
        addresses: vec![],
        fingerprint: String::new(),
        client_id: String::new(),
        paired_at: String::new(),
        last_good_address: None,
        profiles: Default::default(),
    };
    assert_eq!(
        terminal_attach_command(&host, "tmux attach -t ai \\; select-window -t @10").unwrap(),
        "ssh -t xjopa1 'tmux attach -t ai \\; select-window -t @10'"
    );
    assert!(
        terminal_attach_command(&host, "tmux -L aip attach -t 0 \\; select-window -t @2").is_ok()
    );
    for bad in [
        "tmux attach -t ai \\; select-window -t @10; rm -rf ~",
        "tmux attach -t ai \\; select-window -t @10'",
        "tmux attach -t 'a b' \\; select-window -t @10",
        "tmux kill-server",
        "tmux -L $(x) attach -t ai \\; select-window -t @1",
        "sh -c 'x'",
    ] {
        assert!(terminal_attach_command(&host, bad).is_err(), "{bad}");
    }
    let odd = RemoteHost {
        hostname: "-oProxyCommand=x".into(),
        ..host
    };
    assert!(terminal_attach_command(&odd, "tmux attach -t ai \\; select-window -t @10").is_err());
}

#[test]
fn opens_only_sign_in_pages_on_claudes_sites() {
    assert!(sign_in_url_allowed(
        "https://claude.com/cai/oauth/authorize?code=true"
    ));
    assert!(sign_in_url_allowed("https://console.anthropic.com/oauth"));
    assert!(!sign_in_url_allowed("http://claude.com/x"));
    assert!(!sign_in_url_allowed("https://claude.com.evil.example/x"));
    assert!(!sign_in_url_allowed("https://user@claude.com/x"));
    assert!(!sign_in_url_allowed("file:///etc/passwd"));
}

#[tokio::test]
async fn creates_and_deletes_accounts_on_the_host() {
    let server = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let list = list_in(&dir);
    let secrets = MemoryStore::default();
    let host = pair(&list, &secrets, &server.code("s"), None)
        .await
        .unwrap();

    let made = create_account(&list, &secrets, &host.id, " fresh ")
        .await
        .unwrap();
    assert_eq!(made.name, "fresh");
    let taken = create_account(&list, &secrets, &host.id, "fresh")
        .await
        .unwrap_err();
    assert_eq!(code_of(taken), "name_taken");
    let deleted = delete_account(&list, &secrets, &host.id, "fresh")
        .await
        .unwrap();
    assert!(deleted.trashed_to.contains(".trash/fresh-"));
    let gone = delete_account(&list, &secrets, &host.id, "fresh")
        .await
        .unwrap_err();
    assert_eq!(code_of(gone), "not_found");
}

#[test]
fn previews_a_code_and_rejects_nonsense() {
    let code = encode(&PairingCode {
        v: 1,
        hosts: vec!["100.1.2.3:7443".into()],
        secret: "s".into(),
        fp: "ab".repeat(32),
    });
    let preview = preview(&code).unwrap();
    assert_eq!(preview.addresses, vec!["100.1.2.3:7443"]);
    assert!(preview.fingerprint.starts_with("AB:AB:"));
    assert!(matches!(preview_err("hello"), AppError::Validation(_)));
}

fn preview_err(code: &str) -> AppError {
    preview(code).unwrap_err()
}

#[test]
fn renames_within_limits() {
    let dir = tempfile::tempdir().unwrap();
    let list = list_in(&dir);
    assert!(rename(&list, "nope", "x").is_err());
    assert!(rename(&list, "nope", "  ").is_err());
}

#[test]
fn an_ssh_probe_that_gets_in_opens_quietly() {
    assert_eq!(ssh_access(true, ""), SshAccess::Ready);
    assert_eq!(ssh_advice("xjopa1", &SshAccess::Ready).unwrap(), None);
}

#[test]
fn a_host_that_takes_a_password_opens_with_a_hint() {
    let access = ssh_access(
        false,
        "marcus@xjopa1: Permission denied (publickey,password).\n",
    );
    assert_eq!(access, SshAccess::AsksPassword);
    let hint = ssh_advice("xjopa1", &access).unwrap().unwrap();
    assert!(hint.contains("ssh-copy-id xjopa1"), "{hint}");
    assert_eq!(
        ssh_access(false, "Permission denied (publickey,keyboard-interactive)."),
        SshAccess::AsksPassword
    );
}

#[test]
fn a_host_that_takes_keys_only_is_not_opened_and_says_what_to_add() {
    let access = ssh_access(false, "marcus@xjopa1: Permission denied (publickey).\n");
    assert_eq!(access, SshAccess::KeyRefused);
    let message = ssh_advice("xjopa1", &access).unwrap_err().to_string();
    assert!(message.contains("authorized_keys"), "{message}");
}

#[test]
fn a_new_host_key_opens_and_an_unknown_name_does_not() {
    assert_eq!(
        ssh_access(false, "No ED25519 host key is known for xjopa1 and you have requested strict checking.\nHost key verification failed.\n"),
        SshAccess::UnknownHostKey
    );
    assert!(ssh_advice("xjopa1", &SshAccess::UnknownHostKey)
        .unwrap()
        .is_some());
    let access = ssh_access(
        false,
        "ssh: Could not resolve hostname xjopa1: nodename nor servname provided, or not known\n",
    );
    assert!(
        matches!(access, SshAccess::Unreachable(ref said) if said.contains("Could not resolve"))
    );
    assert!(ssh_advice("xjopa1", &access).is_err());
}
