//! Starting and resuming sessions through the API, in a real tmux (on a
//! private socket) running a fake `claude` that logs its arguments and, like
//! the real one, asks about an untrusted folder before registering itself.
//! Skipped where tmux isn't installed.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use ai_profiles_core::api::{ErrorBody, LaunchResult, RemoteSession, WindowScreen};
use ai_profiles_core::tls::pinned_client_config;
use ai_profiles_server::certs::Identity;
use ai_profiles_server::config::Config;
use ai_profiles_server::procs::SystemProcesses;
use ai_profiles_server::routes::ServerState;
use ai_profiles_server::serve::serve;
use ai_profiles_server::store::Store;
use reqwest::StatusCode;

const RESUMABLE: &str = "5d0f1a2b-3c4d-4e5f-8a9b-0c1d2e3f4a5b";
/// Titled by Claude only: nobody named it.
const AUTO_TITLED: &str = "6e1f2a3b-4c5d-4e6f-9a0b-1c2d3e4f5a6b";

/// Acts out the parts of `claude` the server relies on: an optional trust
/// prompt (in folders holding `.untrusted`; one listing pre-approved
/// permissions, taller than the window, with `.preapproved` too), then a
/// registry entry naming its tmux pane, then an empty prompt, where
/// `/rename <name>` renames it in the registry.
const FAKE_CLAUDE: &str = r#"#!/bin/bash
if [ "$1" = "auth" ] && [ "$2" = "logout" ]; then rm -f "$CLAUDE_CONFIG_DIR/.credentials.json"; exit 0; fi
# Answered at once, as the real one does: they're asked before a session starts.
if [ "$1" = "--version" ]; then echo "2.1.0 (Claude Code)"; exit 0; fi
if [ "$1" = "auth" ] && [ "$2" = "status" ]; then
  if [ -e "$CLAUDE_CONFIG_DIR/.credentials.json" ]; then echo '{"loggedIn":true}'; else echo '{"loggedIn":false}'; fi
  exit 0
fi
log="$CLAUDE_CONFIG_DIR/fake-claude.log"
{ echo "START"; echo "CWD=$PWD"; echo "CONFIG=$CLAUDE_CONFIG_DIR"; for arg in "$@"; do echo "ARG=$arg"; done; } >> "$log"
if [ -e "$PWD/.untrusted" ]; then
  selected=no
  draw() {
    clear
    echo "Quick safety check: Is this a project you created or one you trust?"
    if [ -e "$PWD/.preapproved" ]; then
      echo " ⚠ This folder pre-approves 30 tool permissions in .claude/settings.local.json:"
      for n in $(seq 1 29); do echo "   Bash(sudo tool$n:*),"; done
      echo "   Bash(sudo cp:*)"
      echo " These will apply without asking. Only proceed if you trust this configuration."
    fi
    if [ "$selected" = no ]; then echo " ❯ No, exit"; echo "   Yes, I trust this folder"
    else echo "   No, exit"; echo " ❯ Yes, I trust this folder"; fi
  }
  draw
  while IFS= read -rsn1 key; do
    if [ "$key" = $'\x1b' ]; then
      read -rsn2 rest
      case "$rest" in "[B"|"OB") selected=yes ;; esac
      draw
    elif [ -z "$key" ]; then
      [ "$selected" = yes ] && break
      echo "EXITED-UNTRUSTED" >> "$log"; exit 1
    fi
  done
fi
session=$(cat /proc/sys/kernel/random/uuid 2>/dev/null || uuidgen)
while [ $# -gt 0 ]; do [ "$1" = "-r" ] && session="$2"; shift; done
where=$(tmux display-message -p -t "$TMUX_PANE" '#{session_name}:#{window_id}.#{pane_id}')
mkdir -p "$CLAUDE_CONFIG_DIR/sessions"
printf '{"pid":%d,"sessionId":"%s","tmux":"%s","cwd":"%s","startedAt":%s}' $$ "$session" "$where" "$PWD" "$(($(date +%s) * 1000))" > "$CLAUDE_CONFIG_DIR/sessions/$$.json"
echo "READY" >> "$log"
# A resumed session replays its old conversation, Remote Control's last word
# included.
[ -e "$PWD/.rc-disconnected" ] && echo "● Remote Control disconnected — this session was ended or archived from another device"
rule="────────────────────"
printf '%s\n❯ \n%s\n' "$rule" "$rule"
while IFS= read -r line; do
  case "$line" in
    "/rename "*)
      name="${line#/rename }"
      printf '{"pid":%d,"sessionId":"%s","tmux":"%s","cwd":"%s","name":"%s","nameSource":"user","status":"idle"}' $$ "$session" "$where" "$PWD" "$name" > "$CLAUDE_CONFIG_DIR/sessions/$$.json"
      echo "RENAMED=$name" >> "$log"
      printf '%s\n❯ \n%s\n' "$rule" "$rule"
      ;;
  esac
done
sleep 600  # not exec: the process has to go on looking like claude
"#;

fn tmux_installed() -> bool {
    Command::new("tmux")
        .arg("-V")
        .output()
        .is_ok_and(|output| output.status.success())
}

struct Box {
    base: String,
    token: String,
    fingerprint: String,
    root: PathBuf,
    socket: String,
    _home: tempfile::TempDir,
    _stop: tokio::sync::oneshot::Sender<()>,
}

impl Drop for Box {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["-L", &self.socket, "kill-server"])
            .output();
        // kill-server leaves the socket file behind.
        let uid = Command::new("id")
            .arg("-u")
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .unwrap_or_default();
        let dir = std::env::var("TMUX_TMPDIR").unwrap_or_else(|_| "/tmp".into());
        let _ = fs::remove_file(format!("{dir}/tmux-{uid}/{}", self.socket));
    }
}

impl Box {
    fn client(&self) -> reqwest::Client {
        reqwest::Client::builder()
            .use_preconfigured_tls(pinned_client_config(&self.fingerprint).unwrap())
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap()
    }

    fn account(&self) -> PathBuf {
        self.root.join(".claude-accounts/work")
    }

    fn log(&self) -> String {
        fs::read_to_string(self.account().join("fake-claude.log")).unwrap_or_default()
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

    async fn get(&self, path: &str) -> reqwest::Response {
        self.client()
            .get(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .send()
            .await
            .unwrap()
    }

    fn tmux(&self, args: &[&str]) -> String {
        let output = Command::new("tmux")
            .args(["-L", &self.socket])
            .args(args)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

async fn start() -> Box {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().canonicalize().unwrap();
    let claude = root.join("bin/claude");
    write(&claude, FAKE_CLAUDE);
    fs::set_permissions(&claude, fs::Permissions::from_mode(0o755)).unwrap();
    let account = root.join(".claude-accounts/work");
    fs::create_dir_all(&account).unwrap();
    for folder in ["code", "fresh", "odd $(touch pwned) 'name'"] {
        fs::create_dir_all(root.join(folder)).unwrap();
    }
    fs::write(root.join("fresh/.untrusted"), "").unwrap();
    write(
        &account.join(format!("projects/-code/{RESUMABLE}.jsonl")),
        &format!(
            "{}\n{}\n{}\n",
            format_args!(
                r#"{{"type":"user","cwd":"{}"}}"#,
                root.join("code").display()
            ),
            r#"{"type":"assistant"}"#,
            r#"{"type":"custom-title","customTitle":"Billing fix"}"#
        ),
    );
    write(
        &account.join(format!("projects/-code/{AUTO_TITLED}.jsonl")),
        &format!(
            "{}\n{}\n{}\n",
            format_args!(
                r#"{{"type":"user","cwd":"{}"}}"#,
                root.join("code").display()
            ),
            r#"{"type":"assistant"}"#,
            r#"{"type":"ai-title","aiTitle":"Session management tool"}"#
        ),
    );

    let socket = format!(
        "aip-test-{}",
        std::process::id() as u64 * 1000 + rand_suffix()
    );
    let mut config = Config::from_toml(
        &format!(
            "listen = \"127.0.0.1:0\"\ntmux_socket = \"{socket}\"\nclaude_path = \"{}\"\nfolder_roots = [\"{}\"]",
            claude.display(),
            root.display()
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
    let state = Arc::new(ServerState::new(
        config,
        &state_dir,
        Arc::new(SystemProcesses),
    ));
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
    Box {
        base,
        token,
        fingerprint: identity.fingerprint,
        root,
        socket,
        _home: home,
        _stop: stop,
    }
}

fn rand_suffix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as u64
        % 1000
}

#[tokio::test(flavor = "multi_thread")]
async fn starts_a_named_session_with_remote_control_in_the_ai_session() {
    if !tmux_installed() {
        eprintln!("skipped: tmux isn't installed");
        return;
    }
    let server = start().await;
    let cwd = server.root.join("code");
    let response = server
        .post(
            "/v1/accounts/work/sessions",
            serde_json::json!({"cwd": cwd.display().to_string(), "name": "Deploy it", "trustFolder": true}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let launched: LaunchResult = response.json().await.unwrap();
    assert_eq!(launched.window.session, "ai");
    assert!(!launched.already_running);
    assert!(launched.session_id.is_some(), "Claude registered itself");
    assert_eq!(launched.attention, None);
    assert_eq!(launched.remote_control_name.as_deref(), Some("Deploy it"));

    let log = server.log();
    assert!(log.contains(&format!("CWD={}", cwd.display())));
    assert!(log.contains(&format!("CONFIG={}", server.account().display())));
    assert!(log.contains("ARG=--remote-control\nARG=Deploy it\n"));
    let names = server.tmux(&[
        "list-windows",
        "-t",
        "ai",
        "-F",
        "#{window_id} #{window_name}",
    ]);
    assert!(names.contains(&format!("{} Deploy it", launched.window.window_id)));
}

#[tokio::test(flavor = "multi_thread")]
async fn accepts_the_trust_prompt_only_when_asked_to() {
    if !tmux_installed() {
        eprintln!("skipped: tmux isn't installed");
        return;
    }
    let server = start().await;
    let fresh = server.root.join("fresh").display().to_string();

    let untrusted: LaunchResult = server
        .post(
            "/v1/accounts/work/sessions",
            serde_json::json!({"cwd": fresh, "trustFolder": false}),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        untrusted.attention.as_ref().map(|a| a.kind.as_str()),
        Some("trustPrompt")
    );
    assert_eq!(untrusted.session_id, None);

    let trusted: LaunchResult = server
        .post(
            "/v1/accounts/work/sessions",
            serde_json::json!({"cwd": fresh, "trustFolder": true}),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(trusted.attention, None, "the prompt was answered");
    assert!(trusted.session_id.is_some());
    // Unnamed, it's named after its folder, not by Remote Control.
    assert!(server.log().contains("ARG=--remote-control\nARG=fresh\n"));
    assert!(!server.log().contains("remoteControlAtStartup"));
    assert!(
        !server.log().contains("EXITED-UNTRUSTED"),
        "never answered No"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn leaves_trusting_pre_approved_permissions_to_someone_typing_in_the_app() {
    if !tmux_installed() {
        eprintln!("skipped: tmux isn't installed");
        return;
    }
    let server = start().await;
    let risky = server.root.join("risky");
    write(&risky.join(".untrusted"), "");
    write(&risky.join(".preapproved"), "");

    let waiting: LaunchResult = server
        .post(
            "/v1/accounts/work/sessions",
            serde_json::json!({"cwd": risky.display().to_string(), "trustFolder": true}),
        )
        .await
        .json()
        .await
        .unwrap();
    let attention = waiting.attention.expect("left waiting");
    assert_eq!(attention.kind, "trustPermissions");
    assert!(
        attention.text.contains("Bash(sudo cp:*)"),
        "{}",
        attention.text
    );
    assert_eq!(waiting.session_id, None);
    assert!(!server.log().contains("READY"), "not trusted for anyone");

    let window = &waiting.window.window_id;
    let screen: WindowScreen = server
        .get(&format!("/v1/accounts/work/windows/{window}/screen"))
        .await
        .json()
        .await
        .unwrap();
    assert!(screen.text.contains("Yes, I trust this folder"));
    assert!(screen.width > 0 && screen.height > 0);

    let typed = server
        .post(
            &format!("/v1/accounts/work/windows/{window}/keys"),
            serde_json::json!({"keys": [{"key": "Down"}, {"key": "Enter"}]}),
        )
        .await;
    assert_eq!(typed.status(), StatusCode::OK);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !server.log().contains("READY") && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(server.log().contains("READY"), "{}", server.log());
    let started: WindowScreen = server
        .get(&format!("/v1/accounts/work/windows/{window}/screen"))
        .await
        .json()
        .await
        .unwrap();
    assert!(
        started.session_id.is_some(),
        "the screen says it's registered"
    );

    // Only keys from the list, only plain text, only this account's windows.
    let keys = format!("/v1/accounts/work/windows/{window}/keys");
    for bad in [
        serde_json::json!({"keys": [{"key": "kill-server"}]}),
        serde_json::json!({"keys": [{"text": "rm -rf ~\n"}]}),
        serde_json::json!({"keys": []}),
    ] {
        assert_eq!(
            server.post(&keys, bad).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    fs::create_dir_all(server.root.join(".claude-accounts/other")).unwrap();
    for path in [
        format!("/v1/accounts/other/windows/{window}/screen"),
        "/v1/accounts/work/windows/@99999/screen".to_owned(),
    ] {
        assert_eq!(
            server.get(&path).await.status(),
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    let mine = server.tmux(&["new-window", "-d", "-P", "-F", "#{window_id}", "sleep 60"]);
    assert_eq!(
        server
            .get(&format!("/v1/accounts/work/windows/{}/screen", mine.trim()))
            .await
            .status(),
        StatusCode::NOT_FOUND,
        "a window the server didn't open"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn resumes_once_and_then_points_at_the_running_window() {
    if !tmux_installed() {
        eprintln!("skipped: tmux isn't installed");
        return;
    }
    let server = start().await;
    let path = format!("/v1/accounts/work/sessions/{RESUMABLE}/resume");
    let first: LaunchResult = server
        .post(&path, serde_json::json!({"trustFolder": true}))
        .await
        .json()
        .await
        .unwrap();
    assert!(!first.already_running);
    assert_eq!(first.session_id.as_deref(), Some(RESUMABLE));
    assert_eq!(first.remote_control_name.as_deref(), Some("Billing fix"));
    assert!(server.log().contains(&format!(
        "ARG=--remote-control\nARG=Billing fix\nARG=-r\nARG={RESUMABLE}\n"
    )));

    let second: LaunchResult = server
        .post(&path, serde_json::json!({"trustFolder": true}))
        .await
        .json()
        .await
        .unwrap();
    assert!(second.already_running);
    assert_eq!(second.window.window_id, first.window.window_id);
    assert_eq!(server.log().matches("START").count(), 1, "no second copy");

    let sessions: Vec<RemoteSession> = server
        .client()
        .get(format!("{}/v1/accounts/work/sessions", server.base))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let listed = sessions.iter().find(|s| s.id == RESUMABLE).unwrap();
    assert!(listed.running);
    assert_eq!(
        listed.window.as_ref().unwrap().window_id,
        first.window.window_id
    );
}

impl Box {
    async fn sessions(&self) -> Vec<RemoteSession> {
        self.client()
            .get(format!("{}/v1/accounts/work/sessions", self.base))
            .bearer_auth(&self.token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    /// The ids of the windows in the server's tmux.
    fn windows(&self) -> Vec<String> {
        let output = Command::new("tmux")
            .args([
                "-L",
                &self.socket,
                "list-windows",
                "-a",
                "-F",
                "#{window_id}",
            ])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn stops_a_running_session_and_closes_the_window_it_was_started_in() {
    if !tmux_installed() {
        eprintln!("skipped: tmux isn't installed");
        return;
    }
    let server = start().await;
    let resumed: LaunchResult = server
        .post(
            &format!("/v1/accounts/work/sessions/{RESUMABLE}/resume"),
            serde_json::json!({"trustFolder": true}),
        )
        .await
        .json()
        .await
        .unwrap();
    assert!(server.windows().contains(&resumed.window.window_id));

    let stop = format!("/v1/accounts/work/sessions/{RESUMABLE}/stop");
    let stopped: serde_json::Value = server
        .post(&stop, serde_json::json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(stopped["wasRunning"], true);
    let listed = server.sessions().await;
    assert!(!listed.iter().find(|s| s.id == RESUMABLE).unwrap().running);
    assert!(
        !server.windows().contains(&resumed.window.window_id),
        "its window is closed"
    );

    let again: serde_json::Value = server
        .post(&stop, serde_json::json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(again["wasRunning"], false, "nothing left to stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn restarts_a_session_in_a_new_process_and_resumes_one_that_was_not_running() {
    if !tmux_installed() {
        eprintln!("skipped: tmux isn't installed");
        return;
    }
    let server = start().await;
    let restart = format!("/v1/accounts/work/sessions/{RESUMABLE}/restart");
    let first: LaunchResult = server
        .post(&restart, serde_json::json!({"trustFolder": true}))
        .await
        .json()
        .await
        .unwrap();
    assert!(!first.already_running, "not running yet: just resumed");
    assert_eq!(server.log().matches("START").count(), 1);

    let second: LaunchResult = server
        .post(&restart, serde_json::json!({"trustFolder": true}))
        .await
        .json()
        .await
        .unwrap();
    assert!(!second.already_running);
    assert_eq!(second.session_id.as_deref(), Some(RESUMABLE));
    assert_eq!(server.log().matches("START").count(), 2, "a new process");
    // Only the new window is left. Not "the old id is gone": closing the old
    // window can empty the tmux server, and a new one numbers from @0 again.
    assert_eq!(
        server.windows(),
        vec![second.window.window_id.clone()],
        "the old window is gone"
    );
    let listed = server.sessions().await;
    let session = listed.iter().find(|s| s.id == RESUMABLE).unwrap();
    assert!(session.running);
    assert_eq!(
        session.window.as_ref().unwrap().window_id,
        second.window.window_id
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn signs_out_only_once_told_to_stop_what_runs() {
    if !tmux_installed() {
        eprintln!("skipped: tmux isn't installed");
        return;
    }
    let server = start().await;
    let credentials = server.account().join(".credentials.json");
    fs::write(&credentials, "{}").unwrap();
    let resumed = server
        .post(
            &format!("/v1/accounts/work/sessions/{RESUMABLE}/resume"),
            serde_json::json!({"trustFolder": true}),
        )
        .await;
    assert_eq!(resumed.status(), StatusCode::OK);

    let refused = server
        .post("/v1/accounts/work/logout", serde_json::json!({}))
        .await;
    assert_eq!(refused.status(), StatusCode::CONFLICT);
    assert_eq!(
        refused.json::<ErrorBody>().await.unwrap().error.code,
        "sessions_running"
    );
    assert!(credentials.exists(), "still signed in");

    let signed_out = server
        .post(
            "/v1/accounts/work/logout",
            serde_json::json!({"stopRunning": true}),
        )
        .await;
    assert_eq!(signed_out.status(), StatusCode::OK);
    let body: serde_json::Value = signed_out.json().await.unwrap();
    assert_eq!(body["stopped"], 1);
    assert!(!credentials.exists(), "signed out");
    let listed = server.sessions().await;
    assert!(!listed.iter().find(|s| s.id == RESUMABLE).unwrap().running);
}

#[tokio::test(flavor = "multi_thread")]
async fn starts_beside_another_session_in_the_same_folder() {
    if !tmux_installed() {
        eprintln!("skipped: tmux isn't installed");
        return;
    }
    let server = start().await;
    let resumed = server
        .post(
            &format!("/v1/accounts/work/sessions/{RESUMABLE}/resume"),
            serde_json::json!({"trustFolder": true}),
        )
        .await;
    assert_eq!(resumed.status(), StatusCode::OK);

    // Several sessions in one repo is ordinary: nothing to confirm.
    let code = server.root.join("code").display().to_string();
    let started = server
        .post(
            "/v1/accounts/work/sessions",
            serde_json::json!({"cwd": code, "trustFolder": true}),
        )
        .await;
    assert_eq!(started.status(), StatusCode::OK);
    assert_eq!(server.log().matches("START").count(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn passes_odd_folder_and_session_names_through_literally() {
    if !tmux_installed() {
        eprintln!("skipped: tmux isn't installed");
        return;
    }
    let server = start().await;
    let odd = server.root.join("odd $(touch pwned) 'name'");
    let launched: LaunchResult = server
        .post(
            "/v1/accounts/work/sessions",
            serde_json::json!({"cwd": odd.display().to_string(), "name": "a \"quoted\" $(name)", "trustFolder": true}),
        )
        .await
        .json()
        .await
        .unwrap();
    assert!(launched.session_id.is_some());
    let log = server.log();
    assert!(log.contains(&format!("CWD={}", odd.display())));
    assert!(log.contains("ARG=a \"quoted\" $(name)\n"));
    assert!(!odd.join("pwned").exists() && !server.root.join("pwned").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_folders_outside_the_roots_and_unknown_sessions() {
    if !tmux_installed() {
        eprintln!("skipped: tmux isn't installed");
        return;
    }
    let server = start().await;
    let outside = server
        .post(
            "/v1/accounts/work/sessions",
            serde_json::json!({"cwd": "/", "trustFolder": true}),
        )
        .await;
    assert_eq!(outside.status(), StatusCode::FORBIDDEN);
    let unknown = server
        .post(
            "/v1/accounts/work/sessions/00000000-0000-0000-0000-000000000000/resume",
            serde_json::json!({}),
        )
        .await;
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
    let bad = server
        .post(
            "/v1/accounts/work/sessions/not-an-id/resume",
            serde_json::json!({}),
        )
        .await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        bad.json::<ErrorBody>().await.unwrap().error.code,
        "invalid_request"
    );
    assert!(server.log().is_empty(), "nothing was started");
}

#[tokio::test(flavor = "multi_thread")]
async fn renames_a_running_session_through_claude_and_a_stopped_one_in_its_transcript() {
    if !tmux_installed() {
        eprintln!("skipped: tmux isn't installed");
        return;
    }
    let server = start().await;
    let resumed = server
        .post(
            &format!("/v1/accounts/work/sessions/{RESUMABLE}/resume"),
            serde_json::json!({"trustFolder": true}),
        )
        .await;
    assert_eq!(resumed.status(), StatusCode::OK);

    // Running: Claude is asked, at its empty prompt, and its answer is kept.
    let renamed = server
        .post(
            &format!("/v1/accounts/work/sessions/{RESUMABLE}/rename"),
            serde_json::json!({"name": "  Billing, shipped  "}),
        )
        .await;
    assert_eq!(renamed.status(), StatusCode::OK);
    let renamed: serde_json::Value = renamed.json().await.unwrap();
    assert_eq!(renamed["name"], "Billing, shipped");
    assert_eq!(renamed["live"], true);
    assert!(server.log().contains("RENAMED=Billing, shipped\n"));

    let stopped = server
        .post(
            &format!("/v1/accounts/work/sessions/{RESUMABLE}/stop"),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(stopped.status(), StatusCode::OK);

    // Stopped: the transcript gets the title, which the list shows.
    let renamed = server
        .post(
            &format!("/v1/accounts/work/sessions/{RESUMABLE}/rename"),
            serde_json::json!({"name": "Billing archive"}),
        )
        .await;
    assert_eq!(renamed.status(), StatusCode::OK);
    let renamed: serde_json::Value = renamed.json().await.unwrap();
    assert_eq!(renamed["live"], false);
    let sessions: Vec<RemoteSession> = server
        .get("/v1/accounts/work/sessions")
        .await
        .json()
        .await
        .unwrap();
    let session = sessions.iter().find(|s| s.id == RESUMABLE).unwrap();
    assert_eq!(session.title.as_deref(), Some("Billing archive"));
    assert!(session.named);

    let refused = server
        .post(
            &format!("/v1/accounts/work/sessions/{RESUMABLE}/rename"),
            serde_json::json!({"name": "two\nlines"}),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "multi_thread")]
async fn names_remote_control_after_the_title_the_app_lists() {
    if !tmux_installed() {
        eprintln!("skipped: tmux isn't installed");
        return;
    }
    let server = start().await;
    let resumed = server
        .post(
            &format!("/v1/accounts/work/sessions/{AUTO_TITLED}/resume"),
            serde_json::json!({"trustFolder": true}),
        )
        .await;
    assert_eq!(resumed.status(), StatusCode::OK);
    // Not a name Remote Control makes up, which would match nothing else.
    assert!(server
        .log()
        .contains("ARG=--remote-control\nARG=Session management tool\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_just_resumed_session_is_connecting_though_it_replays_an_old_disconnect() {
    if !tmux_installed() {
        eprintln!("skipped: tmux isn't installed");
        return;
    }
    let server = start().await;
    fs::write(server.root.join("code/.rc-disconnected"), "").unwrap();
    let resumed = server
        .post(
            &format!("/v1/accounts/work/sessions/{RESUMABLE}/resume"),
            serde_json::json!({"trustFolder": true}),
        )
        .await;
    assert_eq!(resumed.status(), StatusCode::OK);
    std::thread::sleep(Duration::from_millis(300));
    let sessions: Vec<RemoteSession> = server
        .get("/v1/accounts/work/sessions")
        .await
        .json()
        .await
        .unwrap();
    let session = sessions.iter().find(|s| s.id == RESUMABLE).unwrap();
    assert!(session.running);
    assert!(!session.remote_control);
    assert!(
        session.remote_control_connecting,
        "the disconnect it shows is history"
    );
}
