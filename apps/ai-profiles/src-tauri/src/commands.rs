use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use crate::accounts::{self, AccountStatus};
use crate::app_kind::{spec, AppKind};
use crate::app_state::{self, AppState, AppStatePatch};
use crate::deps::{self, Dependencies};
use crate::error::{AppError, AppResult};
use crate::migration::{
    self, ExistingInstall, ExistingInstallSizes, ImportParams, MigrationBackupInfo,
};
use crate::path_setup::{self, PathHookOutcome, Shell};
use crate::paths::{
    next_migration_backup_dir, profile_dir as profile_data_dir, stock_cli_config_dir,
    stock_gui_support_dir,
};
use crate::profiles::{self, Profile, ProfilePatch, ProfilePaths, Surface, Surfaces};
use crate::remote::hosts::{HostList, RemoteHost};
use crate::remote::{self, secrets, PairingPreview};
use crate::sessions::{
    self, ArchiveCheck, ArchiveReport, ArchivedSession, RestoreCheck, RestoreReport,
    SessionSummary, TransferPlan, TransferReport, TransferRequest,
};
use crate::usage::{
    self,
    codex::CodexQuotaProvider,
    quota::{ClaudeQuotaCache, ClaudeQuotaProvider},
    ProfileUsage,
};
use ai_profiles_core::api::{
    DeletedAccount, DirListing, HostInfo, LaunchResult as RemoteLaunch, LoginStart,
    NewSessionRequest, RemoteAccount, RemoteSession, WindowKey, WindowScreen,
};

#[tauri::command]
pub fn list_profiles() -> AppResult<Vec<Profile>> {
    profiles::load()
}

/// Building a wrapper takes seconds (a copy of the app is cloned and signed), so
/// the commands that can build one run off the main thread, which would
/// otherwise freeze the window for as long. That is what makes the store lock in
/// `profiles` necessary.
///
/// `distinct_dock_icon` is opt-in: leaving it out means the profile gets the
/// plain script launcher. Building a wrapper is something the user chooses.
#[tauri::command(async)]
pub fn create_profile(
    app: AppKind,
    name: String,
    color: String,
    surfaces: Surfaces,
    distinct_dock_icon: Option<bool>,
) -> AppResult<Profile> {
    profiles::create(
        app,
        &name,
        &color,
        surfaces,
        distinct_dock_icon.unwrap_or(false),
    )
}

#[tauri::command]
pub fn regenerate_launchers(id: String) -> AppResult<()> {
    ensure_wrapper_not_running(&id)?;
    let profiles = profiles::load()?;
    let profile = profiles
        .iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| AppError::NotFound(format!("profile {id} not found")))?;
    if profile.surfaces.gui {
        crate::launchers::gui::generate(profile, env!("CARGO_PKG_VERSION"))?;
    }
    if profile.surfaces.cli {
        crate::launchers::cli::generate(profile)?;
    }
    Ok(())
}

/// Every update rebuilds the launcher, whether it renames, recolors or switches
/// its shape, so a wrapper that is running is refused it.
#[tauri::command(async)]
pub fn update_profile(id: String, patch: ProfilePatch) -> AppResult<Profile> {
    ensure_wrapper_not_running(&id)?;
    profiles::update(&id, patch)
}

#[tauri::command]
pub fn delete_profile(id: String, move_to_trash: bool) -> AppResult<()> {
    profiles::delete(&id, move_to_trash)
}

/// Refuse to replace or remove a profile's wrapper while the profile is running
/// from it. The running app *is* that bundle: taking it away leaves the app
/// without files it has yet to load (the helper processes it starts for a new
/// window, say), and a rename would leave the old one behind for good. An id that
/// is not there is left for the operation itself to report.
///
/// Deleting a profile is not refused: it goes away on purpose, and its data goes
/// out from under a running app either way, wrapper or not.
fn ensure_wrapper_not_running(id: &str) -> AppResult<()> {
    let all = profiles::load()?;
    let Some(profile) = all.iter().find(|candidate| candidate.id == id) else {
        return Ok(());
    };
    if crate::launch::running_wrapper(profile)?.is_some() {
        return Err(AppError::Validation(format!(
            "{} ({}) is running. Quit it first.",
            profile.app.spec().display_name,
            profile.name
        )));
    }
    Ok(())
}

#[tauri::command]
pub fn reorder_profiles(ids: Vec<String>) -> AppResult<Vec<Profile>> {
    profiles::reorder(&ids)
}

/// Turning the desktop surface on builds a launcher, and off removes one, which a
/// running wrapper is refused.
#[tauri::command(async)]
pub fn toggle_surface(id: String, surface: Surface, enabled: bool) -> AppResult<Profile> {
    if surface == Surface::Gui && !enabled {
        ensure_wrapper_not_running(&id)?;
    }
    profiles::toggle_surface(&id, surface, enabled)
}

/// What opening a profile's desktop app came to.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResult {
    /// The profile, with its last-used time stamped.
    pub profile: Profile,
    /// Set when the profile asks for a launcher of its own that was left out of
    /// this launch, with why. The setting itself is untouched.
    pub wrapper_bypass: Option<WrapperBypass>,
}

/// Why a profile's own launcher was skipped for one launch.
#[derive(serde::Serialize)]
pub struct WrapperBypass {
    /// A sentence on what went wrong with the launcher.
    pub reason: String,
}

/// Focus the profile's running window if there is one, otherwise launch it: via
/// its `.app` bundle, which carries the tinted icon or, for a profile with its
/// own Dock icon, is the app itself. A launcher that doesn't work never leaves
/// the profile unlaunchable; see [`crate::launch::open_profile`].
///
/// Runs off the main thread, because it waits for the app to come up: a few
/// seconds when macOS is assessing a wrapper it has not seen before, and a minute
/// at the outside.
#[tauri::command(async)]
pub fn open_profile_in_app(app: tauri::AppHandle, id: String) -> AppResult<LaunchResult> {
    let all = profiles::load()?;
    let profile = all
        .iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| AppError::NotFound(format!("profile {id} not found")))?;
    if !profile.surfaces.gui {
        return Err(AppError::Validation("profile has no GUI surface".into()));
    }
    // AppKit wants another app brought forward from the main thread, which this
    // command is not on.
    let focus = |pid: i32| {
        let _ = app.run_on_main_thread(move || crate::launch::focus_pid(pid));
    };
    let bypass = crate::launch::open_profile(profile, env!("CARGO_PKG_VERSION"), focus)?;
    Ok(LaunchResult {
        profile: profiles::touch_last_used(&id)?,
        wrapper_bypass: bypass.map(|bypass| WrapperBypass {
            reason: bypass.to_string(),
        }),
    })
}

/// Stamp `last_used_at` on a profile without launching anything.
///
/// The copy-the-CLI-command action is a "use" of the profile just as much
/// as a desktop launch is, but it happens entirely in the frontend
/// (clipboard write), so it needs an explicit way to record itself. Returns
/// the updated profile so React can patch its cached list in place rather
/// than refetching.
#[tauri::command]
pub fn touch_profile_last_used(id: String) -> AppResult<Profile> {
    profiles::touch_last_used(&id)
}

#[tauri::command]
pub fn open_in_finder(path: String) -> AppResult<()> {
    let target = std::path::Path::new(&path);
    if !target.exists() {
        return Err(AppError::NotFound(format!("path does not exist: {path}")));
    }
    let status = Command::new("open")
        .arg("-R")
        .arg(&path)
        .status()
        .map_err(AppError::Io)?;
    if !status.success() {
        return Err(AppError::Validation(format!(
            "`open -R {path}` exited with status {status}"
        )));
    }
    Ok(())
}

/// Launch — or focus, if already running — the stock desktop app for `app`
/// bound to a specific `--user-data-dir`.
///
/// This is the default entry's counterpart to `open_profile_in_app`. It has no
/// launcher `.app` bundle of its own, so it shells out to the same incantation
/// those bundles use (`open -n -a "<AppName>" --args --user-data-dir=...`), just
/// pointed at the stock data directory.
///
/// `focus_or_launch` provides the single-instance guarantee: neither Claude nor
/// ChatGPT dedupes by data dir (a bare `open -n` would spawn an unbounded number
/// of stock windows), so we detect an existing instance ourselves and focus it
/// instead of launching another.
/// Runs off the main thread, because it waits for the app to come up: the
/// caller keeps its control in an "Opening" state until this returns, and
/// `open` on its own says nothing about there being a window.
#[tauri::command(async)]
pub fn open_default_gui(handle: tauri::AppHandle, app: AppKind, data_dir: String) -> AppResult<()> {
    let app_spec = spec(app);
    // AppKit wants another app brought forward from the main thread, which this
    // command is no longer on.
    let focus = |pid: i32| {
        let _ = handle.run_on_main_thread(move || crate::launch::focus_pid(pid));
    };
    crate::launch::focus_or_launch(&data_dir, app_spec, focus, || {
        crate::launch::open_new_instance(&data_dir, app_spec, None)?;
        crate::launch::wait_for_new_instance(&data_dir, app_spec);
        Ok(())
    })
}

#[tauri::command]
pub fn profile_paths(id: String) -> AppResult<ProfilePaths> {
    profiles::paths(&id)
}

/// The Claude sessions kept by profile `id` (or `default:claude`), newest
/// first. Reads every transcript, so it runs off the main thread.
#[tauri::command(async)]
pub fn list_sessions(id: String) -> AppResult<Vec<SessionSummary>> {
    sessions::list(&sessions::home(&id)?)
}

/// What moving a session would do, without doing it.
#[tauri::command(async)]
pub fn plan_session_transfer(request: TransferRequest) -> AppResult<TransferPlan> {
    sessions::plan(&request)
}

/// Move a session to another profile. See [`sessions::transfer`].
#[tauri::command(async)]
pub fn transfer_session(
    app: tauri::AppHandle,
    request: TransferRequest,
) -> AppResult<TransferReport> {
    use tauri::Emitter;
    // Each step as it starts, for the dialog to show while the move runs.
    sessions::transfer(&request, &|progress| {
        let _ = app.emit(TRANSFER_PROGRESS_EVENT, progress);
    })
}

/// Claude's merge of memory note `path`, which the move `request` needs
/// decided, for the user to accept or not. See [`sessions::merge_with_claude`].
#[tauri::command(async)]
pub fn merge_transfer_memory(request: TransferRequest, path: String) -> AppResult<String> {
    sessions::merge_with_claude(&request, &path)
}

/// The event a move's progress goes out on.
const TRANSFER_PROGRESS_EVENT: &str = "session-transfer-progress";

/// What archiving a session would need: a terminal to close, or the
/// profile's desktop app to quit.
#[tauri::command(async)]
pub fn check_session_archive(profile_id: String, session_id: String) -> AppResult<ArchiveCheck> {
    sessions::check_archive(&profile_id, &session_id)
}

/// Take a session out of a profile, keeping it in `session-transfer-backups`.
/// With `quit_app`, quits the profile's desktop app first when it has to.
#[tauri::command(async)]
pub fn archive_session(
    profile_id: String,
    session_id: String,
    quit_app: bool,
) -> AppResult<ArchiveReport> {
    sessions::archive(&profile_id, &session_id, quit_app)
}

/// The sessions profile `id` has archived, newest first.
#[tauri::command(async)]
pub fn list_archived_sessions(id: String) -> AppResult<Vec<ArchivedSession>> {
    Ok(sessions::list_archived(&sessions::home(&id)?))
}

/// What restoring an archived session would need first.
#[tauri::command(async)]
pub fn check_session_restore(
    profile_id: String,
    session_id: String,
    archive: String,
) -> AppResult<RestoreCheck> {
    sessions::check_restore(&profile_id, &session_id, &archive)
}

/// Delete an archived session for good. Returns what it freed, in bytes.
#[tauri::command(async)]
pub fn delete_archived_session(
    profile_id: String,
    session_id: String,
    archive: String,
) -> AppResult<u64> {
    sessions::delete_archived(&profile_id, &session_id, &archive)
}

/// Put an archived session back. With `quit_app`, quits the profile's desktop
/// app first when its record goes back into that app's list.
#[tauri::command(async)]
pub fn restore_session(
    profile_id: String,
    session_id: String,
    archive: String,
    quit_app: bool,
) -> AppResult<RestoreReport> {
    sessions::restore(&profile_id, &session_id, &archive, quit_app)
}

/// The remote hosts this Mac is paired with.
#[tauri::command(async)]
pub fn remote_list_hosts() -> AppResult<Vec<RemoteHost>> {
    HostList::default_list()?.load()
}

/// What a pairing code says, before pairing with it.
#[tauri::command]
pub fn remote_preview_pairing(code: String) -> AppResult<PairingPreview> {
    remote::preview(&code)
}

/// Pair with the host a code from `ai-profiles-server pair` names.
#[tauri::command]
pub async fn remote_pair_host(code: String, label: Option<String>) -> AppResult<RemoteHost> {
    remote::pair(&HostList::default_list()?, secrets::store(), &code, label).await
}

#[tauri::command(async)]
pub fn remote_rename_host(host_id: String, label: String) -> AppResult<RemoteHost> {
    remote::rename(&HostList::default_list()?, &host_id, &label)
}

/// Forget a host, telling it so when it can be reached.
#[tauri::command]
pub async fn remote_remove_host(host_id: String) -> AppResult<()> {
    remote::remove(&HostList::default_list()?, secrets::store(), &host_id).await
}

#[tauri::command]
pub async fn remote_host_info(host_id: String) -> AppResult<HostInfo> {
    remote::info(&HostList::default_list()?, secrets::store(), &host_id).await
}

#[tauri::command]
pub async fn remote_list_accounts(host_id: String) -> AppResult<Vec<RemoteAccount>> {
    remote::accounts(&HostList::default_list()?, secrets::store(), &host_id).await
}

#[tauri::command]
pub async fn remote_list_sessions(
    host_id: String,
    account: String,
) -> AppResult<Vec<RemoteSession>> {
    remote::sessions(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
    )
    .await
}

#[tauri::command]
pub async fn remote_list_dirs(host_id: String, path: Option<String>) -> AppResult<DirListing> {
    remote::dirs(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        path.as_deref(),
    )
    .await
}

/// Start a Claude session on a remote host, in tmux with Remote Control.
#[tauri::command]
pub async fn remote_new_session(
    host_id: String,
    account: String,
    request: NewSessionRequest,
) -> AppResult<RemoteLaunch> {
    remote::new_session(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
        &request,
    )
    .await
}

/// Resume a remote session in tmux, or find the window it's already in.
#[tauri::command]
pub async fn remote_resume_session(
    host_id: String,
    account: String,
    session_id: String,
    trust_folder: bool,
) -> AppResult<RemoteLaunch> {
    remote::resume(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
        &session_id,
        trust_folder,
    )
    .await
}

/// Rename a remote session: its title, the registry and Remote Control.
#[tauri::command]
pub async fn remote_rename_session(
    host_id: String,
    account: String,
    session_id: String,
    name: String,
) -> AppResult<ai_profiles_core::api::RenameSessionResult> {
    remote::rename_session(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
        &session_id,
        &name,
    )
    .await
}

/// What moving a remote session to another account on its host would do.
#[tauri::command]
pub async fn remote_transfer_plan(
    host_id: String,
    account: String,
    session_id: String,
    to: String,
) -> AppResult<ai_profiles_core::api::TransferPlan> {
    remote::transfer_plan(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
        &session_id,
        &to,
    )
    .await
}

/// Move a remote session to another account on its host.
#[tauri::command]
pub async fn remote_transfer_session(
    host_id: String,
    account: String,
    session_id: String,
    request: ai_profiles_core::api::TransferRequest,
) -> AppResult<ai_profiles_core::api::TransferReport> {
    remote::transfer(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
        &session_id,
        &request,
    )
    .await
}

/// How far a remote move has got; `None` before it starts and once it ends.
#[tauri::command]
pub async fn remote_transfer_progress(
    host_id: String,
    progress_id: String,
) -> AppResult<Option<ai_profiles_core::api::MoveProgress>> {
    remote::transfer_progress(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &progress_id,
    )
    .await
}

/// Ask Claude on the host to merge a memory note a move conflicts on.
#[tauri::command]
pub async fn remote_merge_memory(
    host_id: String,
    account: String,
    session_id: String,
    to: String,
    path: String,
) -> AppResult<String> {
    remote::merge_memory(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
        &session_id,
        &ai_profiles_core::api::MemoryMergeRequest { to, path },
    )
    .await
    .map(|result| result.merged)
}

/// Archive a remote session. Returns where its transcript went.
#[tauri::command]
pub async fn remote_archive_session(
    host_id: String,
    account: String,
    session_id: String,
) -> AppResult<String> {
    remote::archive(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
        &session_id,
    )
    .await
    .map(|result| result.archived_to)
}

/// A remote account's archived sessions.
#[tauri::command]
pub async fn remote_archived_sessions(
    host_id: String,
    account: String,
) -> AppResult<Vec<ai_profiles_core::api::ArchivedSession>> {
    remote::archived(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
    )
    .await
}

/// Delete an archived remote session for good. Returns what it freed, in bytes.
#[tauri::command]
pub async fn remote_delete_archive(
    host_id: String,
    account: String,
    session_id: String,
    archive: String,
) -> AppResult<u64> {
    remote::delete_archive(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
        &session_id,
        &archive,
    )
    .await
    .map(|result| result.freed_bytes)
}

/// Put an archived remote session back.
#[tauri::command]
pub async fn remote_restore_session(
    host_id: String,
    account: String,
    session_id: String,
    archive: String,
) -> AppResult<String> {
    remote::restore(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
        &session_id,
        &archive,
    )
    .await
    .map(|result| result.transcript)
}

/// End a running remote session. `true` if it was running.
#[tauri::command]
pub async fn remote_stop_session(
    host_id: String,
    account: String,
    session_id: String,
) -> AppResult<bool> {
    remote::stop(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
        &session_id,
    )
    .await
    .map(|result| result.was_running)
}

/// Restart a remote session on the host's current `claude`: stop it, then
/// resume it.
#[tauri::command]
pub async fn remote_restart_session(
    host_id: String,
    account: String,
    session_id: String,
    trust_folder: bool,
) -> AppResult<RemoteLaunch> {
    remote::restart(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
        &session_id,
        trust_folder,
    )
    .await
}

#[tauri::command]
pub async fn remote_window_screen(
    host_id: String,
    account: String,
    window_id: String,
) -> AppResult<WindowScreen> {
    remote::window_screen(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
        &window_id,
    )
    .await
}

#[tauri::command]
pub async fn remote_window_keys(
    host_id: String,
    account: String,
    window_id: String,
    keys: Vec<WindowKey>,
) -> AppResult<WindowScreen> {
    remote::window_keys(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
        &window_id,
        keys,
    )
    .await
}

#[tauri::command]
pub async fn remote_create_account(host_id: String, name: String) -> AppResult<RemoteAccount> {
    remote::create_account(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &name,
    )
    .await
}

#[tauri::command]
pub async fn remote_delete_account(host_id: String, account: String) -> AppResult<DeletedAccount> {
    remote::delete_account(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
    )
    .await
}

/// Open a remote session's Remote Control view in the Claude app signed in
/// as `email`, or on claude.ai.
#[tauri::command]
pub async fn remote_open_in_claude(
    email: Option<String>,
    bridge_session_id: String,
) -> AppResult<remote::open_in_claude::Opened> {
    tokio::task::spawn_blocking(move || {
        remote::open_in_claude::open_in_claude(
            email.as_deref(),
            &bridge_session_id,
            env!("CARGO_PKG_VERSION"),
        )
    })
    .await
    .map_err(|err| AppError::Validation(format!("opening it failed: {err}")))?
}

/// Rename a remote profile (its account on the host).
#[tauri::command]
pub async fn remote_rename_account(
    host_id: String,
    account: String,
    new_name: String,
    stop_running: bool,
) -> AppResult<RemoteAccount> {
    remote::rename_account(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
        &new_name,
        stop_running,
    )
    .await
}

/// Give a remote profile its color.
#[tauri::command]
pub fn remote_set_profile_color(
    host_id: String,
    account: String,
    color: String,
) -> AppResult<RemoteHost> {
    remote::set_profile_color(&HostList::default_list()?, &host_id, &account, &color)
}

/// Sign a remote profile out. Returns how many running sessions were stopped.
#[tauri::command]
pub async fn remote_logout(host_id: String, account: String, stop_running: bool) -> AppResult<u32> {
    remote::logout(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
        stop_running,
    )
    .await
}

/// Start signing a remote account in, opening its sign-in page in the browser.
#[tauri::command]
pub async fn remote_login_start(host_id: String, account: String) -> AppResult<LoginStart> {
    remote::start_login(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &account,
        true,
    )
    .await
}

#[tauri::command]
pub async fn remote_login_submit(
    host_id: String,
    login_id: String,
    code: String,
) -> AppResult<RemoteAccount> {
    remote::submit_login(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &login_id,
        &code,
    )
    .await
}

#[tauri::command]
pub async fn remote_login_cancel(host_id: String, login_id: String) -> AppResult<()> {
    remote::cancel_login(
        &HostList::default_list()?,
        secrets::store(),
        &host_id,
        &login_id,
    )
    .await
}

/// Whether profile `id` is signed in, and as whom, read from what its apps
/// keep on disk.
#[tauri::command(async)]
pub fn profile_account(id: String) -> AppResult<AccountStatus> {
    accounts::read(&id)
}

/// Open a web URL (or `mailto:` link) in the user's default handler via
/// macOS's `open` shell command.
///
/// The scheme whitelist is the gate — `open <anything>` would happily
/// launch files, .app bundles, or even custom scheme handlers, so we
/// refuse anything that isn't http(s)/mailto before invoking `open`.
#[tauri::command]
pub fn open_external_url(url: String) -> AppResult<()> {
    if !url.starts_with("https://") && !url.starts_with("http://") && !url.starts_with("mailto:") {
        return Err(AppError::Validation(format!(
            "refusing to open URL with unsupported scheme: {url}"
        )));
    }
    let status = Command::new("open")
        .arg(&url)
        .status()
        .map_err(AppError::Io)?;
    if !status.success() {
        return Err(AppError::Validation(format!(
            "`open {url}` exited with status {status}"
        )));
    }
    Ok(())
}

/// Opens the profile's CLI in a new Terminal window so the user can sign in
/// again (`/login`). Used by the usage card when a token can't be refreshed:
/// running the wrapper interactively is what rotates+persists the credential.
///
/// We resolve the command server-side (the per-profile wrapper `claude-<slug>`,
/// or the stock binary for the default entry) rather than trusting a string
/// from the frontend, then hand it to Terminal via `osascript`.
#[tauri::command]
pub fn open_cli_login(id: String) -> AppResult<()> {
    let all = profiles::load()?;
    let command = cli_login_command(&id, &all)?;
    let script = terminal_applescript(&command);
    let status = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(&script)
        .status()
        .map_err(AppError::Io)?;
    if !status.success() {
        return Err(AppError::Validation(format!(
            "osascript exited with status {status}"
        )));
    }
    Ok(())
}

/// Opens a new Terminal window attached (over ssh) to a remote host's tmux
/// window. The command is built and checked in Rust
/// ([`remote::terminal_attach_command`]), never taken as a shell line.
/// Open a Terminal window attached to a remote tmux window over ssh. Probes
/// ssh first: refuses, saying what to set up, when Terminal would only show
/// ssh failing, and returns a hint when Terminal is going to ask something.
#[tauri::command]
pub async fn remote_open_in_terminal(
    host_id: String,
    attach_command: String,
) -> AppResult<Option<String>> {
    let host = HostList::default_list()?.find(&host_id)?;
    let command = remote::terminal_attach_command(&host, &attach_command)?;
    // Off the async runtime's threads: the probe can take its full timeout.
    let hostname = host.hostname.clone();
    let access = tokio::task::spawn_blocking(move || remote::probe_ssh(&hostname))
        .await
        .map_err(|err| AppError::Validation(format!("the ssh check failed: {err}")))?;
    let hint = remote::ssh_advice(&host.hostname, &access)?;
    let status = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(terminal_applescript(&command))
        .status()
        .map_err(AppError::Io)?;
    if !status.success() {
        return Err(AppError::Validation(format!(
            "osascript exited with status {status}"
        )));
    }
    Ok(hint)
}

/// Pure: the interactive CLI command for a profile entry — the per-profile
/// wrapper (`claude-<slug>`) for managed profiles, or the stock binary
/// (`claude` / `codex`) for the default entry.
fn cli_login_command(id: &str, profiles: &[Profile]) -> AppResult<String> {
    if let Some(kind) = AppKind::from_default_id(id) {
        return Ok(spec(kind).cli_binary.to_string());
    }
    let profile = profiles
        .iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| AppError::NotFound(format!("profile {id} not found")))?;
    Ok(format!(
        "{}-{}",
        profile.app.spec().cli_wrapper_prefix,
        profile.slug
    ))
}

/// Pure: AppleScript that opens a new Terminal window running `command` and
/// brings Terminal to the foreground. `command` is escaped for the AppleScript
/// string literal — defensive only; profile slugs are already a safe charset.
fn terminal_applescript(command: &str) -> String {
    let escaped = command.replace('\\', "\\\\").replace('"', "\\\"");
    format!("tell application \"Terminal\"\n    activate\n    do script \"{escaped}\"\nend tell")
}

#[tauri::command]
pub fn detect_existing_install(app: AppKind) -> AppResult<ExistingInstall> {
    migration::detect_for(app)
}

/// Lazy companion to `detect_existing_install`. The boot-time detection
/// skips the recursive directory walks because they can take 0.5–1s on
/// a large `~/.claude`; the MigrationDialog calls this when it opens so
/// the size column populates a beat later instead of blocking the whole
/// app shell.
#[tauri::command]
pub fn detect_existing_sizes(app: AppKind) -> AppResult<ExistingInstallSizes> {
    let app_spec = spec(app);
    let desktop = stock_gui_support_dir(app_spec)?;
    let code = stock_cli_config_dir(app_spec)?;
    Ok(migration::detect_sizes(&desktop, &code))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportExistingInput {
    pub name: String,
    pub color: String,
    pub include_gui: bool,
    pub include_cli: bool,
}

#[tauri::command]
pub fn import_existing_install(app: AppKind, input: ImportExistingInput) -> AppResult<Profile> {
    let app_spec = spec(app);
    let desktop_path = stock_gui_support_dir(app_spec)?;
    let cli_path = stock_cli_config_dir(app_spec)?;
    let existing = migration::detect(&desktop_path, &cli_path);

    if input.include_gui && existing.gui_path.is_none() {
        return Err(AppError::NotFound(format!(
            "no existing {} Desktop install found",
            app_spec.display_name
        )));
    }
    if input.include_cli && existing.cli_path.is_none() {
        return Err(AppError::NotFound(format!(
            "no existing {} CLI install found",
            app_spec.cli_display_name
        )));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let dir = profile_data_dir(&id)?;
    let backup = next_migration_backup_dir()?;

    let outcome = migration::import(ImportParams {
        id,
        app,
        name: input.name,
        color: input.color,
        include_gui: input.include_gui,
        include_cli: input.include_cli,
        gui_source: input.include_gui.then_some(desktop_path),
        cli_source: input.include_cli.then_some(cli_path),
        profile_dir: dir.clone(),
        backup_dir: backup.clone(),
    })?;

    if outcome.profile.surfaces.gui {
        if let Err(err) =
            crate::launchers::gui::generate(&outcome.profile, env!("CARGO_PKG_VERSION"))
        {
            rollback_import(&outcome.profile, &dir, &backup);
            return Err(err);
        }
    }
    if outcome.profile.surfaces.cli {
        if let Err(err) = crate::launchers::cli::generate(&outcome.profile) {
            if outcome.profile.surfaces.gui {
                let _ = crate::launchers::gui::remove(
                    &outcome.profile.name,
                    outcome.profile.app.spec(),
                );
            }
            rollback_import(&outcome.profile, &dir, &backup);
            return Err(err);
        }
    }

    let _store = profiles::lock_store();
    let mut all = profiles::load()?;
    all.push(outcome.profile.clone());
    if let Err(err) = profiles::save_all(&all) {
        if outcome.profile.surfaces.cli {
            let _ =
                crate::launchers::cli::remove(&outcome.profile.slug, outcome.profile.app.spec());
        }
        if outcome.profile.surfaces.gui {
            let _ =
                crate::launchers::gui::remove(&outcome.profile.name, outcome.profile.app.spec());
        }
        rollback_import(&outcome.profile, &dir, &backup);
        return Err(err);
    }

    Ok(outcome.profile)
}

fn rollback_import(
    profile: &Profile,
    profile_dir_path: &std::path::Path,
    backup: &std::path::Path,
) {
    let app_spec = spec(profile.app);
    if profile.surfaces.gui {
        let backup_gui = backup.join(app_spec.gui_support_dir_name);
        let original = stock_gui_support_dir(app_spec).ok();
        if let (true, Some(target)) = (backup_gui.exists(), original) {
            let _ = std::fs::rename(&backup_gui, &target);
        }
    }
    if profile.surfaces.cli {
        let backup_cli = backup.join(app_spec.cli_stock_config_dir_name);
        let original = stock_cli_config_dir(app_spec).ok();
        if let (true, Some(target)) = (backup_cli.exists(), original) {
            let _ = std::fs::rename(&backup_cli, &target);
        }
    }
    let _ = std::fs::remove_dir_all(backup);
    let _ = std::fs::remove_dir_all(profile_dir_path);
}

#[tauri::command]
pub fn list_migration_backups() -> AppResult<Vec<MigrationBackupInfo>> {
    let root = crate::paths::app_data_dir()?;
    migration::list_backups(&root)
}

#[tauri::command]
pub fn delete_migration_backup(path: String) -> AppResult<()> {
    migration::delete_backup(std::path::Path::new(&path))
}

#[tauri::command]
pub fn check_dependencies() -> AppResult<Dependencies> {
    deps::check_dependencies()
}

#[tauri::command]
pub fn detect_shell() -> Shell {
    Shell::detect_from_env()
}

#[tauri::command]
pub fn install_path_hook(shell: Shell) -> AppResult<PathHookOutcome> {
    let home = dirs::home_dir().ok_or_else(|| AppError::NotFound("home dir unknown".into()))?;
    path_setup::install_path_hook(shell, &home)
}

#[tauri::command]
pub fn load_app_state() -> AppResult<AppState> {
    app_state::load()
}

#[tauri::command]
pub fn update_app_state(patch: AppStatePatch) -> AppResult<AppState> {
    app_state::apply(patch)
}

/// Metadata the About dialog renders.
///
/// Every field is pulled from `Cargo.toml` via Cargo's `env!` macros, so
/// editing the manifest (adding a `repository = "https://github.com/…"`
/// line for example) updates the dialog on next build with no other code
/// changes required.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub repository: Option<String>,
    pub homepage: Option<String>,
    pub license: Option<String>,
}

#[tauri::command]
pub fn get_app_metadata() -> AppMetadata {
    fn optional(value: &str) -> Option<String> {
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    }
    let authors_raw = env!("CARGO_PKG_AUTHORS");
    let authors = authors_raw
        .split(':')
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.to_string())
        .collect();
    AppMetadata {
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: env!("CARGO_PKG_DESCRIPTION").to_string(),
        authors,
        repository: optional(env!("CARGO_PKG_REPOSITORY")),
        homepage: optional(env!("CARGO_PKG_HOMEPAGE")),
        license: optional(env!("CARGO_PKG_LICENSE")),
    }
}

/// One shared refresher across all `get_profile_usage` invocations so
/// its per-profile mutex + backoff registry survives across calls.
/// A new instance per command would defeat both: two simultaneous
/// commands on the same profile would race, and a 5-minute auto-refetch
/// would never see the previous "tried at" timestamp.
static CLAUDE_REFRESHER: OnceLock<usage::refresh::ClaudeCliRefresher> = OnceLock::new();
/// One shared usage cache across all `get_profile_usage` invocations so the
/// 5-minute success cache and the 429 back-off survive between calls. A new
/// instance per command would defeat both.
static CLAUDE_QUOTA_CACHE: OnceLock<ClaudeQuotaCache> = OnceLock::new();
/// One shared dead-credential registry across all `get_profile_usage`
/// invocations, so a token marked "needs login" stays marked between calls
/// (until the user re-auths and the access token rotates). Keyed per token
/// hash, not per profile.
static CLAUDE_DEAD_CREDS: OnceLock<usage::dead_credentials::DeadCredentialRegistry> =
    OnceLock::new();

#[tauri::command]
pub async fn get_profile_usage(profile_id: String) -> AppResult<ProfileUsage> {
    let app = resolve_app(&profile_id)?;
    let config_dir = resolve_cli_config_dir(&profile_id)?;
    let app_spec = spec(app);
    if !app_spec.has_usage {
        return Ok(ProfileUsage {
            quota: None,
            quota_error: None,
            fetched_at: chrono::Utc::now().to_rfc3339(),
        });
    }
    let user_agent = format!("ai-profiles/{}", env!("CARGO_PKG_VERSION"));
    match app {
        AppKind::Claude => {
            let cache = CLAUDE_QUOTA_CACHE.get_or_init(ClaudeQuotaCache::new);
            let dead_credentials =
                CLAUDE_DEAD_CREDS.get_or_init(usage::dead_credentials::DeadCredentialRegistry::new);
            let provider = ClaudeQuotaProvider::new(user_agent, cache, dead_credentials)
                .map_err(|_| AppError::Io(std::io::Error::other("could not build HTTP client")))?;
            let refresher = CLAUDE_REFRESHER.get_or_init(usage::refresh::ClaudeCliRefresher::new);
            Ok(
                usage::build_with_cli_refresh(&config_dir, &provider, refresher, dead_credentials)
                    .await,
            )
        }
        AppKind::Codex => {
            // app-server refreshes its own token per call, so no external
            // refresher dance is needed.
            let provider = CodexQuotaProvider::new(
                "ai-profiles".to_string(),
                env!("CARGO_PKG_VERSION").to_string(),
            );
            Ok(usage::build(&config_dir, &provider).await)
        }
    }
}

fn resolve_app(profile_id: &str) -> AppResult<AppKind> {
    if let Some(kind) = AppKind::from_default_id(profile_id) {
        return Ok(kind);
    }
    let all = profiles::load()?;
    all.into_iter()
        .find(|profile| profile.id == profile_id)
        .map(|profile| profile.app)
        .ok_or_else(|| AppError::NotFound(format!("profile {profile_id} not found")))
}

fn resolve_cli_config_dir(profile_id: &str) -> AppResult<PathBuf> {
    if let Some(kind) = AppKind::from_default_id(profile_id) {
        return stock_cli_config_dir(spec(kind));
    }
    let profile_root = profile_data_dir(profile_id)?;
    Ok(profile_root.join("cli-config"))
}

#[cfg(test)]
mod usage_routing_tests {
    use super::*;

    #[test]
    fn resolve_cli_config_dir_for_default_claude_points_at_dot_claude() {
        let resolved = resolve_cli_config_dir("default:claude").expect("home resolvable");
        assert!(resolved.ends_with(".claude"));
        let parent = resolved.parent().expect("has parent");
        assert_eq!(parent, dirs::home_dir().unwrap().as_path());
    }

    #[test]
    fn resolve_cli_config_dir_for_default_codex_points_at_dot_codex() {
        let resolved = resolve_cli_config_dir("default:codex").expect("home resolvable");
        assert!(resolved.ends_with(".codex"));
    }

    #[test]
    fn resolve_cli_config_dir_for_managed_id_is_per_profile() {
        let resolved = resolve_cli_config_dir("some-managed-id").expect("ok");
        assert!(resolved.ends_with("cli-config"));
    }
}

#[cfg(test)]
mod cli_login_tests {
    use super::*;

    fn managed(id: &str, app: AppKind, slug: &str) -> Profile {
        Profile {
            id: id.into(),
            app,
            name: "X".into(),
            slug: slug.into(),
            color: "#000000".into(),
            created_at: "2026-06-14T00:00:00Z".into(),
            surfaces: Surfaces {
                gui: false,
                cli: true,
            },
            distinct_dock_icon: false,
            last_used_at: None,
        }
    }

    #[test]
    fn cli_login_command_uses_the_wrapper_for_a_managed_profile() {
        let profiles = vec![managed("abc", AppKind::Claude, "personal")];
        assert_eq!(
            cli_login_command("abc", &profiles).unwrap(),
            "claude-personal"
        );
    }

    #[test]
    fn cli_login_command_uses_the_codex_prefix_for_a_codex_profile() {
        let profiles = vec![managed("xyz", AppKind::Codex, "work")];
        assert_eq!(cli_login_command("xyz", &profiles).unwrap(), "codex-work");
    }

    #[test]
    fn cli_login_command_uses_the_stock_binary_for_default_entries() {
        assert_eq!(cli_login_command("default:claude", &[]).unwrap(), "claude");
        assert_eq!(cli_login_command("default:codex", &[]).unwrap(), "codex");
    }

    #[test]
    fn cli_login_command_is_not_found_for_an_unknown_id() {
        assert!(matches!(
            cli_login_command("nope", &[]).unwrap_err(),
            AppError::NotFound(_)
        ));
    }

    #[test]
    fn terminal_applescript_runs_the_command_and_activates() {
        let script = terminal_applescript("claude-personal");
        assert!(script.contains(r#"do script "claude-personal""#));
        assert!(script.contains("activate"));
    }

    #[test]
    fn terminal_applescript_escapes_quotes_and_backslashes() {
        // Defensive: a stray quote must not break out of the string literal.
        let script = terminal_applescript(r#"a"b\c"#);
        assert!(script.contains(r#"do script "a\"b\\c""#));
    }
}

#[cfg(test)]
mod running_wrapper_tests {
    use super::*;
    use crate::test_support::{fake_wrapper_process, APP_DIR_TEST_LOCK};

    fn profile(id: &str, distinct_dock_icon: bool) -> Profile {
        Profile {
            id: id.into(),
            app: AppKind::Claude,
            name: "Work".into(),
            slug: "work".into(),
            color: "#000000".into(),
            created_at: "2026-06-14T00:00:00Z".into(),
            surfaces: Surfaces {
                gui: true,
                cli: false,
            },
            distinct_dock_icon,
            last_used_at: None,
        }
    }

    fn purge() {
        let _ = std::fs::remove_dir_all(crate::paths::app_data_dir().unwrap());
    }

    #[test]
    fn a_wrapper_that_is_running_is_refused_until_it_quits() {
        let _guard = APP_DIR_TEST_LOCK.lock().unwrap();
        purge();
        let wrapped = profile("aaaaaaaa-0000-0000-0000-000000000007", true);
        profiles::save_all(std::slice::from_ref(&wrapped)).unwrap();
        let root = tempfile::tempdir().unwrap();
        let data_dir = profile_data_dir(&wrapped.id).unwrap().join("gui-data");

        let mut process = fake_wrapper_process(root.path(), &data_dir);
        let while_running = ensure_wrapper_not_running(&wrapped.id);
        process.kill().unwrap();
        process.wait().unwrap();
        let after = ensure_wrapper_not_running(&wrapped.id);
        purge();

        match while_running {
            Err(AppError::Validation(message)) => assert!(message.contains(&wrapped.name)),
            other => panic!("expected a refusal, got {other:?}"),
        }
        after.unwrap();
    }

    #[test]
    fn a_profile_that_is_not_running_from_a_wrapper_is_not_refused() {
        let _guard = APP_DIR_TEST_LOCK.lock().unwrap();
        purge();
        let plain = profile("bbbbbbbb-0000-0000-0000-000000000007", false);
        profiles::save_all(std::slice::from_ref(&plain)).unwrap();
        let root = tempfile::tempdir().unwrap();
        let data_dir = profile_data_dir(&plain.id).unwrap().join("gui-data");

        // Something on its data dir that looks like a wrapper does not make it
        // one: only a profile that asks for a wrapper is ever running from one.
        let mut process = fake_wrapper_process(root.path(), &data_dir);
        let plain_result = ensure_wrapper_not_running(&plain.id);
        process.kill().unwrap();
        process.wait().unwrap();
        // An id that isn't there is for the operation itself to report.
        let unknown_result = ensure_wrapper_not_running("no-such-profile");
        purge();

        plain_result.unwrap();
        unknown_result.unwrap();
    }
}
