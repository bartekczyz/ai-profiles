//! Remote hosts: Linux machines running ai-profiles-server, whose Claude
//! accounts and sessions the app shows next to the local profiles.
//!
//! The webview never talks to a host. Every call goes through here, so the
//! token stays in Rust and the Keychain, and the app's CSP stays IPC-only.

pub mod client;
pub mod hosts;
pub mod open_in_claude;
pub mod secrets;

use std::process::Command;

use ai_profiles_core::api::{
    ArchiveResult, ArchivedSession, CreateAccountRequest, DeleteArchiveResult, DeletedAccount,
    DirListing, HostInfo, LaunchResult, LoginCodeRequest, LoginStart, LogoutRequest, LogoutResult,
    MemoryMergeRequest, MemoryMergeResult, MoveProgress, NewSessionRequest, PairRequest,
    PairResponse, RemoteAccount, RemoteSession, RenameAccountRequest, RenameSessionRequest,
    RenameSessionResult, RestoreResult, ResumeRequest, StopResult, TransferPlan, TransferReport,
    TransferRequest, WindowKey, WindowKeysRequest, WindowScreen,
};
use ai_profiles_core::pairing::{self, display_fingerprint};
use serde::Serialize;

use self::client::{remote_error, Answer, HostClient};
use self::hosts::{HostList, RemoteHost};
use self::secrets::SecretStore;
use crate::error::{AppError, AppResult};

/// What a pairing code says, shown before pairing so the user can compare the
/// fingerprint with what `ai-profiles-server pair` printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingPreview {
    pub addresses: Vec<String>,
    /// `AB:CD:…`
    pub fingerprint: String,
}

fn read_code(code: &str) -> AppResult<pairing::PairingCode> {
    pairing::decode(code).map_err(|err| AppError::Validation(err.to_string()))
}

pub fn preview(code: &str) -> AppResult<PairingPreview> {
    let code = read_code(code)?;
    Ok(PairingPreview {
        addresses: code.hosts,
        fingerprint: display_fingerprint(&code.fp),
    })
}

/// This Mac's name, as the server's `clients` list shows it.
fn this_mac() -> String {
    Command::new("scutil")
        .args(["--get", "ComputerName"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "ai-profiles".into())
}

/// Pair with the host a code names: trade its secret for a token over a
/// connection pinned to its certificate, keep the token in `secrets`, and
/// add (or, when already paired with that certificate, refresh) the host.
pub async fn pair(
    list: &HostList,
    secrets: &dyn SecretStore,
    code: &str,
    label: Option<String>,
) -> AppResult<RemoteHost> {
    let code = read_code(code)?;
    let existing = list
        .load()?
        .into_iter()
        .find(|host| host.fingerprint == code.fp);
    let candidate = RemoteHost {
        id: existing
            .as_ref()
            .map(|host| host.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        label: "the server".into(),
        hostname: String::new(),
        addresses: code.hosts.clone(),
        fingerprint: code.fp.clone(),
        client_id: String::new(),
        paired_at: chrono::Utc::now().to_rfc3339(),
        last_good_address: None,
        // Pairing again keeps how its profiles look.
        profiles: existing
            .as_ref()
            .map(|host| host.profiles.clone())
            .unwrap_or_default(),
    };
    let Answer {
        value: paired,
        address,
    } = HostClient::new(candidate.clone(), None)?
        .post::<_, PairResponse>(
            "/v1/pair",
            &PairRequest {
                secret: code.secret,
                client_name: this_mac(),
            },
        )
        .await?;

    let label = label
        .map(|label| label.trim().to_owned())
        .filter(|label| !label.is_empty())
        .or_else(|| existing.as_ref().map(|host| host.label.clone()))
        .unwrap_or_else(|| paired.info.hostname.clone());
    let host = RemoteHost {
        label,
        hostname: paired.info.hostname,
        client_id: paired.client_id,
        last_good_address: Some(address),
        ..candidate
    };
    if let Err(error) = secrets.set(&host.id, &paired.token) {
        // Without the token this pairing is useless; don't leave the server
        // holding a client that can never come back.
        if let Ok(client) = HostClient::new(host.clone(), Some(paired.token)) {
            let _ = client.delete("/v1/clients/self").await;
        }
        return Err(error);
    }
    if existing.is_some() {
        list.update(&host.id, |saved| *saved = host.clone())?;
    } else {
        list.add(host.clone())?;
    }
    Ok(host)
}

/// Forget a host: tell it this client is gone (best effort: it may be off),
/// then drop the token and the entry.
pub async fn remove(list: &HostList, secrets: &dyn SecretStore, host_id: &str) -> AppResult<()> {
    if let Ok((_, client)) = connect(list, secrets, host_id) {
        let _ = client.delete("/v1/clients/self").await;
    }
    secrets.delete(host_id)?;
    list.remove(host_id)?;
    Ok(())
}

pub fn rename(list: &HostList, host_id: &str, label: &str) -> AppResult<RemoteHost> {
    let label = label.trim();
    if label.is_empty() || label.chars().count() > 64 {
        return Err(AppError::Validation(
            "a host's name must be 1–64 characters".into(),
        ));
    }
    list.update(host_id, |host| host.label = label.to_owned())
}

fn connect(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
) -> AppResult<(RemoteHost, HostClient)> {
    let host = list.find(host_id)?;
    let token = secrets.get(host_id)?.ok_or_else(|| {
        remote_error(
            "unauthorized",
            format!(
                "The token for {} is missing from the Keychain. Pair it again.",
                host.label
            ),
        )
    })?;
    let client = HostClient::new(host.clone(), Some(token))?;
    Ok((host, client))
}

/// Note which address answered, so it's tried first next time.
fn remember(list: &HostList, host: &RemoteHost, address: &str) {
    if host.last_good_address.as_deref() != Some(address) {
        let _ = list.update(&host.id, |saved| {
            saved.last_good_address = Some(address.to_owned())
        });
    }
}

async fn get<T: serde::de::DeserializeOwned>(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    path: &str,
    query: &[(&str, &str)],
) -> AppResult<T> {
    let (host, client) = connect(list, secrets, host_id)?;
    let answer = client.get(path, query).await?;
    remember(list, &host, &answer.address);
    Ok(answer.value)
}

async fn post<B: Serialize, T: serde::de::DeserializeOwned>(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    path: &str,
    body: &B,
) -> AppResult<T> {
    let (host, client) = connect(list, secrets, host_id)?;
    let answer = client.post(path, body).await?;
    remember(list, &host, &answer.address);
    Ok(answer.value)
}

pub async fn info(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
) -> AppResult<HostInfo> {
    get(list, secrets, host_id, "/v1/info", &[]).await
}

pub async fn accounts(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
) -> AppResult<Vec<RemoteAccount>> {
    get(list, secrets, host_id, "/v1/accounts", &[]).await
}

/// Account names are folder names the server validated; anything else is
/// refused here rather than escaped into a URL.
fn account_segment(account: &str) -> AppResult<&str> {
    let plain = !account.is_empty()
        && account.len() <= 64
        && account
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        && !account.starts_with('.');
    plain
        .then_some(account)
        .ok_or_else(|| AppError::Validation(format!("{account:?} isn't an account name")))
}

pub async fn sessions(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
) -> AppResult<Vec<RemoteSession>> {
    let path = format!("/v1/accounts/{}/sessions", account_segment(account)?);
    get(list, secrets, host_id, &path, &[]).await
}

/// The subfolders of `path` on the host (its home when `None`).
pub async fn dirs(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    path: Option<&str>,
) -> AppResult<DirListing> {
    let query: Vec<(&str, &str)> = path.map(|path| ("path", path)).into_iter().collect();
    get(list, secrets, host_id, "/v1/fs/dirs", &query).await
}

/// Start a session for `account` in `request.cwd`, in the host's tmux.
pub async fn new_session(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
    request: &NewSessionRequest,
) -> AppResult<LaunchResult> {
    let path = format!("/v1/accounts/{}/sessions", account_segment(account)?);
    post(list, secrets, host_id, &path, request).await
}

/// `/v1/accounts/<account>/sessions/<session_id>/<action>`, once both are
/// checked to be plain.
fn session_path(account: &str, session_id: &str, action: &str) -> AppResult<String> {
    Ok(format!(
        "/v1/accounts/{}/sessions/{}/{action}",
        account_segment(account)?,
        uuid_segment(session_id)?
    ))
}

/// `id`, once checked to be plain hex and dashes, as session ids are.
fn uuid_segment(id: &str) -> AppResult<&str> {
    let plain =
        !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-');
    if plain {
        Ok(id)
    } else {
        Err(AppError::Validation(format!("{id:?} isn't a session id")))
    }
}

/// Resume a session in the host's tmux, or find the window it's running in.
pub async fn resume(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
    session_id: &str,
    trust_folder: bool,
) -> AppResult<LaunchResult> {
    let path = session_path(account, session_id, "resume")?;
    post(
        list,
        secrets,
        host_id,
        &path,
        &ResumeRequest { trust_folder },
    )
    .await
}

/// End a running session on the host.
pub async fn stop(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
    session_id: &str,
) -> AppResult<StopResult> {
    let path = session_path(account, session_id, "stop")?;
    post(list, secrets, host_id, &path, &serde_json::json!({})).await
}

/// Give a session on the host a name, which its transcript, the registry and
/// Remote Control all take.
pub async fn rename_session(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
    session_id: &str,
    name: &str,
) -> AppResult<RenameSessionResult> {
    let path = session_path(account, session_id, "rename")?;
    post(
        list,
        secrets,
        host_id,
        &path,
        &RenameSessionRequest {
            name: name.to_owned(),
        },
    )
    .await
}

/// Stop a session and resume it on the host's current `claude`.
pub async fn restart(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
    session_id: &str,
    trust_folder: bool,
) -> AppResult<LaunchResult> {
    let path = session_path(account, session_id, "restart")?;
    post(
        list,
        secrets,
        host_id,
        &path,
        &ResumeRequest { trust_folder },
    )
    .await
}

/// What moving a session to account `to` on the host would do.
pub async fn transfer_plan(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
    session_id: &str,
    to: &str,
) -> AppResult<TransferPlan> {
    let path = session_path(account, session_id, "transfer")?;
    get(
        list,
        secrets,
        host_id,
        &path,
        &[("to", account_segment(to)?)],
    )
    .await
}

/// Move a session to another account on the host, with the plan's
/// questions answered.
pub async fn transfer(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
    session_id: &str,
    request: &TransferRequest,
) -> AppResult<TransferReport> {
    account_segment(&request.to)?;
    let path = session_path(account, session_id, "transfer")?;
    let (host, client) = connect(list, secrets, host_id)?;
    // Stopping the session, copying a long conversation and starting it again
    // can take a while.
    let answer = client
        .patient(TRANSFER_TIMEOUT)
        .post(&path, request)
        .await?;
    remember(list, &host, &answer.address);
    Ok(answer.value)
}

/// How long a move is given on the host.
const TRANSFER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

/// How far the move the client called `progress_id` has got; `None` once it
/// has ended, or before it has started.
pub async fn transfer_progress(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    progress_id: &str,
) -> AppResult<Option<MoveProgress>> {
    let path = format!("/v1/moves/{}", uuid_segment(progress_id)?);
    match get(list, secrets, host_id, &path, &[]).await {
        Ok(progress) => Ok(Some(progress)),
        Err(AppError::Remote { code, .. }) if code == "not_found" => Ok(None),
        Err(error) => Err(error),
    }
}

/// How long Claude is given to merge a memory note on the host.
const MERGE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

/// Ask Claude on the host to merge a memory note a move conflicts on.
pub async fn merge_memory(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
    session_id: &str,
    request: &MemoryMergeRequest,
) -> AppResult<MemoryMergeResult> {
    account_segment(&request.to)?;
    let path = session_path(account, session_id, "transfer/merge-memory")?;
    let (host, client) = connect(list, secrets, host_id)?;
    let answer = client.patient(MERGE_TIMEOUT).post(&path, request).await?;
    remember(list, &host, &answer.address);
    Ok(answer.value)
}

/// Archive a session on the host: its transcript moves aside.
pub async fn archive(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
    session_id: &str,
) -> AppResult<ArchiveResult> {
    let path = session_path(account, session_id, "archive")?;
    post(list, secrets, host_id, &path, &serde_json::json!({})).await
}

/// An account's archived sessions on the host.
pub async fn archived(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
) -> AppResult<Vec<ArchivedSession>> {
    let path = format!("/v1/accounts/{}/archived", account_segment(account)?);
    get(list, secrets, host_id, &path, &[]).await
}

/// Put an archived session back on the host.
pub async fn restore(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
    session_id: &str,
    archive: &str,
) -> AppResult<RestoreResult> {
    let plain_archive = archive.ends_with("-archived")
        && archive
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b'-' || b.is_ascii_lowercase());
    if !plain_archive {
        return Err(AppError::Validation(format!(
            "{archive:?} isn't an archive"
        )));
    }
    // Checks the account and session id.
    session_path(account, session_id, "archive")?;
    let path = format!(
        "/v1/accounts/{}/archived/{session_id}/{archive}/restore",
        account_segment(account)?
    );
    post(list, secrets, host_id, &path, &serde_json::json!({})).await
}

/// `/v1/accounts/<account>/archived/<session_id>/<archive>`, once all three
/// are checked to be plain.
fn archive_path(account: &str, session_id: &str, archive: &str) -> AppResult<String> {
    let plain_archive = archive.ends_with("-archived")
        && archive
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b'-' || b.is_ascii_lowercase());
    if !plain_archive {
        return Err(AppError::Validation(format!(
            "{archive:?} isn't an archive"
        )));
    }
    Ok(format!(
        "/v1/accounts/{}/archived/{}/{archive}",
        account_segment(account)?,
        uuid_segment(session_id)?
    ))
}

/// Delete an archived session on the host for good. Returns what it freed.
pub async fn delete_archive(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
    session_id: &str,
    archive: &str,
) -> AppResult<DeleteArchiveResult> {
    let path = archive_path(account, session_id, archive)?;
    let (host, client) = connect(list, secrets, host_id)?;
    let answer = client.delete_for(&path).await?;
    remember(list, &host, &answer.address);
    Ok(answer.value)
}

/// Make an account on the host.
pub async fn create_account(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    name: &str,
) -> AppResult<RemoteAccount> {
    let name = name.trim();
    post(
        list,
        secrets,
        host_id,
        "/v1/accounts",
        &CreateAccountRequest {
            name: name.to_owned(),
        },
    )
    .await
}

/// Move an account to the host's trash.
pub async fn delete_account(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
) -> AppResult<DeletedAccount> {
    let path = format!("/v1/accounts/{}", account_segment(account)?);
    let (host, client) = connect(list, secrets, host_id)?;
    let answer = client.delete_for(&path).await?;
    remember(list, &host, &answer.address);
    // The profile goes with it.
    list.update(host_id, |host| {
        host.profiles.remove(account);
    })?;
    Ok(answer.value)
}

/// Rename an account on the host, stopping its running sessions first when
/// `stop_running`, and carry its profile's look over to the new name.
pub async fn rename_account(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
    new_name: &str,
    stop_running: bool,
) -> AppResult<RemoteAccount> {
    let path = format!("/v1/accounts/{}/rename", account_segment(account)?);
    let renamed: RemoteAccount = post(
        list,
        secrets,
        host_id,
        &path,
        &RenameAccountRequest {
            new_name: new_name.to_owned(),
            stop_running,
        },
    )
    .await?;
    if renamed.name != account {
        list.update(host_id, |host| {
            if let Some(look) = host.profiles.remove(account) {
                host.profiles.insert(renamed.name.clone(), look);
            }
        })?;
    }
    Ok(renamed)
}

/// Give a remote profile its color.
pub fn set_profile_color(
    list: &HostList,
    host_id: &str,
    account: &str,
    color: &str,
) -> AppResult<RemoteHost> {
    account_segment(account)?;
    let hex = color.len() == 7
        && color.starts_with('#')
        && color[1..].bytes().all(|b| b.is_ascii_hexdigit());
    if !hex {
        return Err(AppError::Validation(format!(
            "{color:?} isn't a #rrggbb color"
        )));
    }
    list.update(host_id, |host| {
        host.profiles.insert(
            account.to_owned(),
            hosts::RemoteProfileLook {
                color: color.to_ascii_lowercase(),
            },
        );
    })
}

/// Sign a remote account out, stopping its running sessions first when
/// `stop_running`. Returns how many were stopped.
pub async fn logout(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
    stop_running: bool,
) -> AppResult<u32> {
    let path = format!("/v1/accounts/{}/logout", account_segment(account)?);
    let result: LogoutResult = post(
        list,
        secrets,
        host_id,
        &path,
        &LogoutRequest { stop_running },
    )
    .await?;
    Ok(result.stopped)
}

/// Where the sign-in page may be. The server checks too; this side checks
/// again before opening anything in the browser.
const SIGN_IN_HOSTS: &[&str] = &[
    "claude.com",
    "claude.ai",
    "platform.claude.com",
    "console.anthropic.com",
];

pub fn sign_in_url_allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    !host.contains('@') && !host.contains(':') && SIGN_IN_HOSTS.contains(&host)
}

/// Start signing an account in on the host, and open its sign-in page here.
pub async fn start_login(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
    open_page: bool,
) -> AppResult<LoginStart> {
    let path = format!("/v1/accounts/{}/login", account_segment(account)?);
    let started: LoginStart = post(list, secrets, host_id, &path, &serde_json::json!({})).await?;
    if !sign_in_url_allowed(&started.url) {
        return Err(remote_error(
            "login_failed",
            "The host offered a sign-in link to an unexpected site, so it wasn't opened.",
        ));
    }
    if open_page {
        let _ = Command::new("open").arg(&started.url).status();
    }
    Ok(started)
}

/// How long handing over a sign-in code may take: the host gives claude up to
/// 30s to finish signing in, then up to 15s to confirm it, so the usual 30s
/// would give up while it's still answering.
const SUBMIT_LOGIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Hand the code from the sign-in page to the host.
pub async fn submit_login(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    login_id: &str,
    code: &str,
) -> AppResult<RemoteAccount> {
    let path = format!("/v1/logins/{}/code", login_segment(login_id)?);
    let (host, client) = connect(list, secrets, host_id)?;
    let answer = client
        .patient(SUBMIT_LOGIN_TIMEOUT)
        .post(
            &path,
            &LoginCodeRequest {
                code: code.trim().to_owned(),
            },
        )
        .await?;
    remember(list, &host, &answer.address);
    Ok(answer.value)
}

pub async fn cancel_login(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    login_id: &str,
) -> AppResult<()> {
    let path = format!("/v1/logins/{}", login_segment(login_id)?);
    let (_, client) = connect(list, secrets, host_id)?;
    client.delete(&path).await.map(|_| ())
}

/// What a window the host opened shows now.
pub async fn window_screen(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
    window_id: &str,
) -> AppResult<WindowScreen> {
    let path = format!(
        "/v1/accounts/{}/windows/{}/screen",
        account_segment(account)?,
        window_segment(window_id)?
    );
    get(list, secrets, host_id, &path, &[]).await
}

/// Type into a window the host opened; answers with what it shows after.
pub async fn window_keys(
    list: &HostList,
    secrets: &dyn SecretStore,
    host_id: &str,
    account: &str,
    window_id: &str,
    keys: Vec<WindowKey>,
) -> AppResult<WindowScreen> {
    let path = format!(
        "/v1/accounts/{}/windows/{}/keys",
        account_segment(account)?,
        window_segment(window_id)?
    );
    post(list, secrets, host_id, &path, &WindowKeysRequest { keys }).await
}

/// Whether `ssh` from this Mac gets into a host, as a probe that asks nothing
/// (`BatchMode`) finds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SshAccess {
    /// In without asking anything.
    Ready,
    /// The host would take a password: Terminal can ask for it.
    AsksPassword,
    /// This Mac hasn't seen the host's key: Terminal asks to confirm it.
    UnknownHostKey,
    /// The host takes keys only, and none of this Mac's.
    KeyRefused,
    /// Not reachable at all; what ssh said.
    Unreachable(String),
}

/// Probe `hostname` (already checked to be a plain name) with ssh.
pub fn probe_ssh(hostname: &str) -> SshAccess {
    let output = std::process::Command::new("/usr/bin/ssh")
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=5",
            "--",
            hostname,
            "true",
        ])
        .output();
    match output {
        Ok(output) => ssh_access(
            output.status.success(),
            &String::from_utf8_lossy(&output.stderr),
        ),
        Err(err) => SshAccess::Unreachable(format!("could not run ssh: {err}")),
    }
}

/// Pure: what a `BatchMode` probe's outcome says about getting in.
pub fn ssh_access(success: bool, stderr: &str) -> SshAccess {
    if success {
        return SshAccess::Ready;
    }
    if stderr.contains("Host key verification failed") {
        return SshAccess::UnknownHostKey;
    }
    if let Some(methods) = stderr
        .split("Permission denied (")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
    {
        let asks = methods
            .split(',')
            .any(|method| method == "password" || method == "keyboard-interactive");
        return if asks {
            SshAccess::AsksPassword
        } else {
            SshAccess::KeyRefused
        };
    }
    let said = stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("ssh failed")
        .to_owned();
    SshAccess::Unreachable(said)
}

/// What to tell the user about opening a Terminal on `hostname`, given the
/// probe: `Ok(None)` to open it quietly, `Ok(Some(hint))` to open it and say
/// what it will ask, `Err` not to open it at all, saying why.
pub fn ssh_advice(hostname: &str, access: &SshAccess) -> AppResult<Option<String>> {
    match access {
        SshAccess::Ready => Ok(None),
        SshAccess::AsksPassword => Ok(Some(format!(
            "Terminal will ask for {hostname}'s password. To skip that next time, run ssh-copy-id {hostname} once."
        ))),
        SshAccess::UnknownHostKey => Ok(Some(format!(
            "This is this Mac's first ssh to {hostname}: Terminal asks you to confirm its host key."
        ))),
        SshAccess::KeyRefused => Err(AppError::Validation(format!(
            "{hostname} only accepts SSH keys, and none of this Mac's. Add this Mac's public key (~/.ssh/id_ed25519.pub; ssh-keygen makes one) to ~/.ssh/authorized_keys on {hostname}."
        ))),
        SshAccess::Unreachable(said) => Err(AppError::Validation(format!(
            "ssh can't reach {hostname} from this Mac ({said}). If it's a name only you use, add it to ~/.ssh/config."
        ))),
    }
}

/// The shell command that attaches to a host's tmux window over ssh, for a
/// Terminal window on this Mac. Built here from checked parts rather than
/// taken from the host: `attach` has to be exactly the server's
/// `tmux [-L <socket>] attach -t <session> \; select-window -t @<n>`, and
/// the host name a plain name, so nothing the host says can add to what
/// runs here.
pub fn terminal_attach_command(host: &RemoteHost, attach: &str) -> AppResult<String> {
    fn plain(word: &str) -> bool {
        !word.is_empty()
            && word.len() <= 64
            && word
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    }
    let hostname_ok = !host.hostname.is_empty()
        && host.hostname.len() <= 253
        && !host.hostname.starts_with(['-', '.'])
        && host
            .hostname
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.');
    let words: Vec<&str> = attach.split(' ').collect();
    let (socket, rest) = match words.as_slice() {
        ["tmux", "-L", socket, rest @ ..] => (Some(*socket), rest),
        ["tmux", rest @ ..] => (None, rest),
        _ => (None, &[][..]),
    };
    let attach_ok = match rest {
        ["attach", "-t", session, "\\;", "select-window", "-t", window] => {
            plain(session)
                && window.len() > 1
                && window.starts_with('@')
                && window[1..].bytes().all(|b| b.is_ascii_digit())
        }
        _ => false,
    };
    if !hostname_ok || !attach_ok || !socket.is_none_or(plain) {
        return Err(AppError::Validation(
            "That isn't a tmux window to open in Terminal.".into(),
        ));
    }
    Ok(format!("ssh -t {} '{attach}'", host.hostname))
}

/// A tmux window id: `@` and digits.
fn window_segment(window_id: &str) -> AppResult<&str> {
    let plain = window_id.len() > 1
        && window_id.len() <= 10
        && window_id.starts_with('@')
        && window_id[1..].bytes().all(|b| b.is_ascii_digit());
    plain
        .then_some(window_id)
        .ok_or_else(|| AppError::Validation(format!("{window_id:?} isn't a tmux window")))
}

fn login_segment(login_id: &str) -> AppResult<&str> {
    let plain = !login_id.is_empty()
        && login_id.len() <= 64
        && login_id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-');
    plain
        .then_some(login_id)
        .ok_or_else(|| AppError::Validation("that isn't a sign-in id".into()))
}

#[cfg(test)]
mod tests;
