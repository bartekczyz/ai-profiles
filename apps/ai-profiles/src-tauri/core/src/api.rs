//! The HTTP API between ai-profiles (client) and ai-profiles-server, as
//! types both sides compile. JSON is camelCase.
//!
//! Versioning: routes live under `/v1`. Adding a field or a route keeps
//! [`API_VERSION`] where it is and clients check `apiVersion` before using
//! anything newer; a change that breaks an existing shape moves to `/v2`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::account::ProfileAccount;
use crate::memory::{Decision, MemoryAction, Side};
use crate::session_move::ItemAction;

pub const API_VERSION: u32 = 1;

/// Sent by the client with every request, `ai-profiles/<version>`.
pub const CLIENT_HEADER: &str = "x-aip-client";
/// Sent by the server with every response: its [`API_VERSION`].
pub const API_HEADER: &str = "x-aip-api";

/// `GET /v1/ping`, the one route that needs no token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ping {
    pub api_version: u32,
}

/// `POST /v1/pair`: trade a pairing code's one-time secret for a token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairRequest {
    pub secret: String,
    /// Shown by `ai-profiles-server clients`, e.g. the Mac's name.
    pub client_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairResponse {
    pub client_id: String,
    /// The bearer token for every later request. Shown once, never stored by
    /// the server (it keeps a hash).
    pub token: String,
    pub info: HostInfo,
}

/// `GET /v1/info`: what the host is and what it can do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    pub hostname: String,
    /// The home folder, so the client can shorten paths to `~`.
    pub home: String,
    pub server_version: String,
    pub api_version: u32,
    /// `None` when tmux isn't installed: sessions can be listed but not started.
    pub tmux: Option<TmuxInfo>,
    /// `None` when no `claude` binary was found.
    pub claude: Option<ClaudeInfo>,
    pub accounts_base: String,
    pub includes_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TmuxInfo {
    pub version: String,
    /// The tmux session new Claude sessions open in, e.g. `ai`.
    pub session: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeInfo {
    pub path: String,
    pub version: Option<String>,
}

/// One Claude account on the host: a `CLAUDE_CONFIG_DIR`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAccount {
    /// The folder's name under the accounts base, or `default` for `~/.claude`.
    pub name: String,
    pub is_default: bool,
    pub config_dir: String,
    /// Whose account it is, when it has been signed in.
    pub account: Option<ProfileAccount>,
    pub signed_in: bool,
    /// When the sign-in runs out (RFC 3339), if Claude recorded it. It renews
    /// the short-lived access token until then; after it, the profile has to
    /// sign in again.
    #[serde(default)]
    pub signed_in_until: Option<String>,
    /// Sessions with something in them.
    pub sessions: u32,
    pub running_sessions: u32,
}

/// One session of an account, as the client's list shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSession {
    pub id: String,
    /// The folder it last worked in.
    pub cwd: Option<String>,
    /// Its name, else Claude's generated title.
    pub title: Option<String>,
    /// Someone chose the name (so Remote Control uses it).
    pub named: bool,
    pub last_prompt: Option<String>,
    /// When the transcript was last written, RFC 3339.
    pub updated_at: String,
    pub size_bytes: u64,
    pub running: bool,
    /// Where it runs in tmux, when it is running there.
    pub window: Option<TmuxWindow>,
    /// Remote Control is connected.
    pub remote_control: bool,
    /// The session's id on claude.ai while Remote Control is connected
    /// (`session_…`): `claude://code/<id>` opens it in the Claude app, and
    /// `https://claude.ai/code/<id>` on the web.
    #[serde(default)]
    pub bridge_session_id: Option<String>,
    /// Its window is open but Claude hasn't started it yet: it's asking
    /// something first (whether to trust the folder), which someone has to
    /// answer in the window.
    #[serde(default)]
    pub waiting: bool,
    /// Started by the server, with Remote Control, moments ago, and Remote
    /// Control hasn't connected yet.
    #[serde(default)]
    pub remote_control_connecting: bool,
    /// Nothing has been said in it yet: listed only while it runs, since
    /// once stopped there's nothing to resume.
    #[serde(default)]
    pub empty: bool,
    /// The Claude Code version it runs, while it runs.
    #[serde(default)]
    pub claude_version: Option<String>,
    /// A newer Claude Code is installed than the one it runs: Claude shows
    /// "Update installed · Restart to update", and a restart takes it on.
    #[serde(default)]
    pub update_pending: bool,
}

/// A tmux window running a Claude session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TmuxWindow {
    pub session: String,
    /// `@7`: the id to target, stable for the window's lifetime.
    pub window_id: String,
    pub pane_id: String,
}

/// `POST /v1/accounts/{name}/sessions`: start a session in a folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionRequest {
    pub cwd: String,
    /// Names the session, and so its Remote Control session. Without one,
    /// Remote Control names it after the folder.
    #[serde(default)]
    pub name: Option<String>,
    /// Accept Claude's "do you trust this folder?" prompt for it. Remote
    /// Control can't connect until the folder is trusted.
    #[serde(default)]
    pub trust_folder: bool,
}

/// `POST /v1/accounts/{name}/sessions/{id}/resume`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeRequest {
    #[serde(default)]
    pub trust_folder: bool,
}

/// `POST /v1/accounts/{name}/rename`: give the account another name, which
/// is its folder's. Unique on the server, ignoring case.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameAccountRequest {
    pub new_name: String,
    /// Stop the account's running sessions first: they have its folder open.
    /// Refused with `sessions_running` while any run otherwise.
    #[serde(default)]
    pub stop_running: bool,
}

/// `POST /v1/accounts/{name}/sessions/{id}/rename`: give a session a name,
/// which its transcript, the registry and Remote Control all take.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameSessionRequest {
    pub name: String,
}

/// What a session was renamed to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameSessionResult {
    /// The name it has now. Claude adds a suffix when another live session
    /// on the host holds the one asked for.
    pub name: String,
    /// It was running, so Claude took the name now, Remote Control included;
    /// otherwise Remote Control takes it when the session is resumed.
    pub live: bool,
}

/// `POST /v1/accounts/{name}/logout`: sign the account out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogoutRequest {
    /// Stop the account's running sessions first. Refused with
    /// `sessions_running` while any run otherwise.
    #[serde(default)]
    pub stop_running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogoutResult {
    /// Sessions stopped on the way.
    pub stopped: u32,
}

/// `POST /v1/accounts/{name}/sessions/{id}/stop`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopResult {
    /// It was running, and now isn't. `false`: there was nothing to stop.
    pub was_running: bool,
}

/// Where a started (or already running) session is, and anything it's
/// waiting for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResult {
    pub window: TmuxWindow,
    /// It was running already: this is its window, nothing was started.
    pub already_running: bool,
    /// Known once Claude has registered the session (a new session's id is
    /// Claude's to choose).
    pub session_id: Option<String>,
    /// The name Remote Control shows it under, when it was given one.
    pub remote_control_name: Option<String>,
    /// The window is waiting for someone at the keyboard.
    pub attention: Option<Attention>,
    /// `tmux attach -t <session> \; select-window -t <window>`, to run on the
    /// host (the client adds the ssh).
    pub attach_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attention {
    /// `trustPrompt` (the folder wasn't trusted and trusting it wasn't
    /// asked for), `trustPermissions` (trusting it would also pre-approve
    /// the tool permissions its settings list, which the server never
    /// accepts for anyone) or `waiting` (something else).
    pub kind: String,
    /// The last lines on screen, for the person who'll deal with it.
    pub text: String,
}

/// `GET /v1/accounts/{name}/windows/{window}/screen`, and the answer to
/// sending keys: what a window the server opened shows now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowScreen {
    /// The visible screen, one line per row.
    pub text: String,
    pub width: u16,
    pub height: u16,
    /// Set once Claude in the window has registered its session: it has
    /// got past whatever it asked on the way.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Remote Control is connected.
    #[serde(default)]
    pub remote_control: bool,
}

/// One thing to type into a window: a named key (tmux's names: `Enter`,
/// `Down`, `C-c`, …) or plain text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowKey {
    Key(String),
    Text(String),
}

/// `POST /v1/accounts/{name}/windows/{window}/keys`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowKeysRequest {
    pub keys: Vec<WindowKey>,
}

/// `POST /v1/accounts`: make an account (a new `CLAUDE_CONFIG_DIR`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAccountRequest {
    pub name: String,
}

/// `DELETE /v1/accounts/{name}`: where the account's folder went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedAccount {
    pub trashed_to: String,
}

/// `POST /v1/accounts/{name}/login`: a sign-in in progress on the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginStart {
    pub login_id: String,
    /// The page to sign in at, which then shows a code to paste back.
    pub url: String,
    /// RFC 3339: after this the sign-in is abandoned.
    pub expires_at: String,
}

/// `POST /v1/logins/{id}/code`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginCodeRequest {
    pub code: String,
}

/// `GET /v1/fs/dirs?path=`: the subfolders of a folder, for picking where a
/// new session works.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    pub path: String,
    /// The folder above, when it is still inside the allowed roots.
    pub parent: Option<String>,
    pub home: String,
    pub entries: Vec<DirEntry>,
    /// More subfolders exist than were returned.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub path: String,
}

/// Every non-2xx response: `{"error": {"code": …, "message": …}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorDetail {
    /// Stable, snake_case: `unauthorized`, `not_found`, `forbidden_path`,
    /// `invalid_request`, `rate_limited`, `internal`, …
    pub code: String,
    /// For people: one sentence, safe to show as is.
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_are_camel_case_on_the_wire() {
        let session = RemoteSession {
            id: "s".into(),
            cwd: None,
            title: Some("T".into()),
            named: true,
            last_prompt: None,
            updated_at: "2026-01-01T00:00:00Z".into(),
            size_bytes: 1,
            running: true,
            window: Some(TmuxWindow {
                session: "ai".into(),
                window_id: "@1".into(),
                pane_id: "%1".into(),
            }),
            remote_control: true,
            bridge_session_id: Some("session_01abc".into()),
            waiting: false,
            remote_control_connecting: false,
            empty: false,
            claude_version: None,
            update_pending: false,
        };
        let json = serde_json::to_value(&session).unwrap();
        assert_eq!(json["lastPrompt"], serde_json::Value::Null);
        assert_eq!(json["window"]["windowId"], "@1");
        assert_eq!(json["remoteControl"], true);
        assert_eq!(json["bridgeSessionId"], "session_01abc");
        let back: RemoteSession = serde_json::from_value(json).unwrap();
        assert_eq!(back, session);
    }
}

/// A running Claude that a start, resume or move would run beside.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningMatch {
    pub account: String,
    pub pid: i32,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    /// The very session: two copies would write two versions of it.
    pub exact: bool,
    pub window: Option<TmuxWindow>,
}

/// One item of a move's plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferItem {
    /// Relative to the destination account.
    pub path: String,
    pub action: ItemAction,
}

/// One project memory file of a move's plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferMemoryFile {
    /// Relative to the project's memory folder.
    pub path: String,
    pub action: MemoryAction,
    pub newer: Side,
    /// Both texts, for a conflict, to show the difference.
    pub source_text: Option<String>,
    pub destination_text: Option<String>,
}

/// `GET /v1/accounts/{name}/sessions/{id}/transfer?to=<account>`: what
/// moving the session would do, and what it needs decided first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferPlan {
    pub session_id: String,
    pub source: String,
    pub destination: String,
    pub title: Option<String>,
    pub cwd: Option<String>,
    /// In order, transcript last. `same` items are listed too.
    pub items: Vec<TransferItem>,
    /// The destination's copy of the transcript differs and was written
    /// later: the move would roll it back, so it needs `replaceNewer`.
    pub destination_newer: bool,
    pub memory: Vec<TransferMemoryFile>,
    /// Claude already running with this session, in either account: the
    /// move needs `stopFirst` (or `confirmRunning`, to move beside it).
    pub running: Vec<RunningMatch>,
    /// What deleting the source's copy afterwards (`deleteSource`) frees, in
    /// bytes.
    #[serde(default)]
    pub source_bytes: u64,
    /// What archiving the source's copy afterwards takes, in bytes, before
    /// compression: the transcript, which is all an archive holds.
    #[serde(default)]
    pub archive_bytes: u64,
}

/// `GET /v1/accounts/{name}/sessions/{id}/transfer` query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferQuery {
    pub to: String,
}

/// `POST /v1/accounts/{name}/sessions/{id}/transfer`: move it, with the
/// plan's questions answered.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequest {
    pub to: String,
    /// Stop the session first if it is running (in either account), so the
    /// move takes the whole conversation and nothing writes to it meanwhile.
    #[serde(default)]
    pub stop_first: bool,
    #[serde(default)]
    pub replace_newer: bool,
    #[serde(default)]
    pub confirm_running: bool,
    /// One per memory conflict, by path.
    #[serde(default)]
    pub memory: HashMap<String, Decision>,
    /// Archive the source's transcript afterwards, so only the destination
    /// lists the session.
    #[serde(default)]
    pub archive_source: bool,
    /// Delete the source's copy afterwards instead, once the moved copy is
    /// checked to be identical: frees its space, and can't be undone. Not
    /// with `archiveSource`.
    #[serde(default)]
    pub delete_source: bool,
    /// Resume it under the destination afterwards, in tmux.
    #[serde(default)]
    pub resume: bool,
    #[serde(default)]
    pub trust_folder: bool,
    /// Chosen by the client, to follow the move with
    /// `GET /v1/moves/{progressId}` while it runs.
    #[serde(default)]
    pub progress_id: Option<String>,
}

/// `GET /v1/moves/{progressId}`: how far a move has got.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveProgress {
    /// What it does, in order.
    pub steps: Vec<String>,
    /// The step it's on now, an index into `steps`.
    pub current: usize,
}

/// What a move did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferReport {
    /// Anything was copied, replaced or removed; `false` when the destination
    /// was already up to date.
    pub changed: bool,
    /// Where replaced destination files went.
    pub backup_dir: Option<String>,
    /// One line per memory file that was looked at, as claudemulti prints them.
    pub memory: Vec<String>,
    pub archived_to: Option<String>,
    /// What deleting the source's copy freed, in bytes, when asked for.
    #[serde(default)]
    pub freed_bytes: Option<u64>,
    /// Why the source's copy wasn't deleted, when that was asked for: the
    /// move itself is done, and the copy is kept.
    #[serde(default)]
    pub delete_error: Option<String>,
    /// The resumed session, when asked for.
    pub launch: Option<LaunchResult>,
    /// Why resuming didn't work, when it was asked for and failed: the move
    /// itself is done.
    pub resume_error: Option<String>,
}

/// `POST /v1/accounts/{name}/sessions/{id}/transfer/merge-memory`: ask
/// Claude, under the destination account, to merge one conflicting note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMergeRequest {
    pub to: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMergeResult {
    pub merged: String,
}

/// `POST /v1/accounts/{name}/sessions/{id}/archive`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveResult {
    pub archived_to: String,
}

/// One archived session, from `GET /v1/accounts/{name}/archived`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedSession {
    pub id: String,
    /// `<stamp>-archived`: which archive, since a session can be archived
    /// more than once.
    pub archive: String,
    /// `%Y%m%d-%H%M%S`, the host's local time.
    pub stamp: String,
    pub title: Option<String>,
    pub cwd: Option<String>,
    /// What the archive takes on disk, in bytes.
    #[serde(default)]
    pub size_bytes: u64,
}

/// `DELETE /v1/accounts/{name}/archived/{id}/{archive}`: an archive deleted
/// for good.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteArchiveResult {
    /// What it freed, in bytes.
    pub freed_bytes: u64,
}

/// `POST /v1/accounts/{name}/archived/{id}/{archive}/restore`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub transcript: String,
}
