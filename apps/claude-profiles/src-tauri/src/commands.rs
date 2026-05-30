use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use crate::activity::{self, Activity, ActivityKind};
use crate::app_state::{self, AppState, AppStatePatch};
use crate::deps::{self, Dependencies};
use crate::error::{AppError, AppResult};
use crate::migration::{
    self, ExistingInstall, ExistingInstallSizes, ImportParams, MigrationBackupInfo,
};
use crate::path_setup::{self, PathHookOutcome, Shell};
use crate::paths::{
    activity_log_path, claude_code_install_path, claude_desktop_install_path, gui_launcher_path,
    next_migration_backup_dir, profile_dir as profile_data_dir,
};
use crate::profiles::{self, Profile, ProfilePatch, ProfilePaths, Surface, Surfaces};
use crate::usage::{self, quota::ReqwestUsageClient, ProfileUsage};

/// Append an activity entry to the profile's log. Failures are logged
/// but do not propagate — the log is best-effort, not load-bearing.
fn record_silent(profile_id: &str, kind: ActivityKind, metadata: Option<serde_json::Value>) {
    let path = match activity_log_path(profile_id) {
        Ok(path) => path,
        Err(_) => return,
    };
    let _ = activity::append(&path, &Activity::now(kind, metadata));
}

#[tauri::command]
pub fn list_profiles() -> AppResult<Vec<Profile>> {
    profiles::load()
}

#[tauri::command]
pub fn create_profile(name: String, color: String, surfaces: Surfaces) -> AppResult<Profile> {
    let profile = profiles::create(&name, &color, surfaces)?;
    record_silent(&profile.id, ActivityKind::Created, None);
    Ok(profile)
}

#[tauri::command]
pub fn regenerate_launchers(id: String) -> AppResult<()> {
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

#[tauri::command]
pub fn update_profile(id: String, patch: ProfilePatch) -> AppResult<Profile> {
    // Capture the prior state so we can attribute the activity precisely
    // (renamed vs. color_changed) and emit one entry per change.
    let before = profiles::load()?
        .into_iter()
        .find(|profile| profile.id == id)
        .ok_or_else(|| AppError::NotFound(format!("profile {id} not found")))?;
    let updated = profiles::update(&id, patch)?;
    if updated.name != before.name {
        record_silent(
            &updated.id,
            ActivityKind::Renamed,
            Some(serde_json::json!({ "from": before.name, "to": updated.name })),
        );
    }
    if updated.color.to_lowercase() != before.color.to_lowercase() {
        record_silent(
            &updated.id,
            ActivityKind::ColorChanged,
            Some(serde_json::json!({ "from": before.color, "to": updated.color })),
        );
    }
    Ok(updated)
}

#[tauri::command]
pub fn delete_profile(id: String, move_to_trash: bool) -> AppResult<()> {
    profiles::delete(&id, move_to_trash)
}

#[tauri::command]
pub fn reorder_profiles(ids: Vec<String>) -> AppResult<Vec<Profile>> {
    profiles::reorder(&ids)
}

#[tauri::command]
pub fn toggle_surface(id: String, surface: Surface, enabled: bool) -> AppResult<Profile> {
    let profile = profiles::toggle_surface(&id, surface, enabled)?;
    record_silent(
        &profile.id,
        ActivityKind::SurfaceToggled,
        Some(serde_json::json!({
            "surface": match surface { Surface::Gui => "gui", Surface::Cli => "cli" },
            "enabled": enabled,
        })),
    );
    Ok(profile)
}

#[tauri::command]
pub fn open_profile_in_app(id: String) -> AppResult<Profile> {
    let all = profiles::load()?;
    let profile = all
        .iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| AppError::NotFound(format!("profile {id} not found")))?;
    if !profile.surfaces.gui {
        return Err(AppError::Validation("profile has no GUI surface".into()));
    }
    // Single-instance gate: focus the profile's running window if there is
    // one, otherwise launch via its `.app` bundle (which carries the tinted
    // icon). The bundle's data dir matches the launcher script's
    // `--user-data-dir`, so detection lines up with what actually runs.
    let data_dir = profile_data_dir(&id)?.join("gui-data");
    let app_path = gui_launcher_path(&profile.name);
    crate::launch::focus_or_launch(&data_dir.display().to_string(), || {
        let status = Command::new("open")
            .arg(&app_path)
            .status()
            .map_err(AppError::Io)?;
        if !status.success() {
            return Err(AppError::Validation(format!(
                "`open {}` exited with status {status}",
                app_path.display()
            )));
        }
        Ok(())
    })?;
    record_silent(&id, ActivityKind::LaunchedGui, None);
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

/// Launch — or focus, if already running — the stock Claude desktop app bound
/// to a specific `--user-data-dir`.
///
/// This is the default entry's counterpart to `open_profile_in_app`. It has no
/// launcher `.app` bundle of its own, so it shells out to the same incantation
/// those bundles use (`open -n -a "Claude" --args --user-data-dir=...`), just
/// pointed at the stock data directory.
///
/// `focus_or_launch` provides the single-instance guarantee: Claude does not
/// dedupe by data dir (a bare `open -n` would spawn an unbounded number of
/// stock windows), so we detect an existing instance ourselves and focus it
/// instead of launching another.
#[tauri::command]
pub fn open_claude_gui(data_dir: String) -> AppResult<()> {
    crate::launch::focus_or_launch(&data_dir, || crate::launch::open_new_instance(&data_dir))
}

#[tauri::command]
pub fn profile_paths(id: String) -> AppResult<ProfilePaths> {
    profiles::paths(&id)
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

#[tauri::command]
pub fn list_activity(profile_id: String, limit: usize) -> AppResult<Vec<Activity>> {
    let path = activity_log_path(&profile_id)?;
    activity::read_last_n(&path, limit)
}

#[tauri::command]
pub fn record_activity(
    profile_id: String,
    kind: ActivityKind,
    metadata: Option<serde_json::Value>,
) -> AppResult<Profile> {
    record_silent(&profile_id, kind, metadata);
    // Usage kinds (gui launch, cli copy) also stamp last_used_at; other
    // kinds return the current profile state unchanged. React invalidates
    // the profiles cache on the returned value either way.
    match kind {
        ActivityKind::LaunchedGui | ActivityKind::CopiedCli => {
            profiles::touch_last_used(&profile_id)
        }
        _ => profiles::load()?
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| AppError::NotFound(format!("profile {profile_id} not found"))),
    }
}

#[tauri::command]
pub fn detect_existing_claude_install() -> AppResult<ExistingInstall> {
    let desktop = claude_desktop_install_path()?;
    let code = claude_code_install_path()?;
    Ok(migration::detect(&desktop, &code))
}

/// Lazy companion to `detect_existing_claude_install`. The boot-time
/// detection skips the recursive directory walks because they can take
/// 0.5–1s on a large `~/.claude`; the MigrationDialog calls this when
/// it opens so the size column populates a beat later instead of
/// blocking the whole app shell.
#[tauri::command]
pub fn detect_existing_claude_sizes() -> AppResult<ExistingInstallSizes> {
    let desktop = claude_desktop_install_path()?;
    let code = claude_code_install_path()?;
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
pub fn import_existing_install(input: ImportExistingInput) -> AppResult<Profile> {
    let desktop_path = claude_desktop_install_path()?;
    let cli_path = claude_code_install_path()?;
    let existing = migration::detect(&desktop_path, &cli_path);

    if input.include_gui && existing.claude_desktop_path.is_none() {
        return Err(AppError::NotFound(
            "no existing Claude Desktop install found".into(),
        ));
    }
    if input.include_cli && existing.claude_code_path.is_none() {
        return Err(AppError::NotFound(
            "no existing Claude Code install found".into(),
        ));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let dir = profile_data_dir(&id)?;
    let backup = next_migration_backup_dir()?;

    let outcome = migration::import(ImportParams {
        id,
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
                let _ = crate::launchers::gui::remove(&outcome.profile.name);
            }
            rollback_import(&outcome.profile, &dir, &backup);
            return Err(err);
        }
    }

    let mut all = profiles::load()?;
    all.push(outcome.profile.clone());
    if let Err(err) = profiles::save_all(&all) {
        if outcome.profile.surfaces.cli {
            let _ = crate::launchers::cli::remove(&outcome.profile.slug);
        }
        if outcome.profile.surfaces.gui {
            let _ = crate::launchers::gui::remove(&outcome.profile.name);
        }
        rollback_import(&outcome.profile, &dir, &backup);
        return Err(err);
    }

    record_silent(
        &outcome.profile.id,
        ActivityKind::Imported,
        Some(serde_json::json!({ "backupDir": outcome.backup_dir })),
    );
    Ok(outcome.profile)
}

fn rollback_import(
    profile: &Profile,
    profile_dir_path: &std::path::Path,
    backup: &std::path::Path,
) {
    if profile.surfaces.gui {
        let backup_gui = backup.join("Claude");
        let original = claude_desktop_install_path().ok();
        if let (true, Some(target)) = (backup_gui.exists(), original) {
            let _ = std::fs::rename(&backup_gui, &target);
        }
    }
    if profile.surfaces.cli {
        let backup_cli = backup.join(".claude");
        let original = claude_code_install_path().ok();
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
static REFRESHER: OnceLock<usage::refresh::ClaudeCliRefresher> = OnceLock::new();

#[tauri::command]
pub async fn get_profile_usage(profile_id: String) -> AppResult<ProfileUsage> {
    let cli_config = resolve_cli_config_dir(&profile_id)?;
    let client = ReqwestUsageClient::new(format!("claude-profiles/{}", env!("CARGO_PKG_VERSION")))
        .map_err(|_| AppError::Io(std::io::Error::other("could not build HTTP client")))?;
    let refresher = REFRESHER.get_or_init(usage::refresh::ClaudeCliRefresher::new);
    Ok(usage::build_with_cli_refresh(&cli_config, &client, refresher).await)
}

fn resolve_cli_config_dir(profile_id: &str) -> AppResult<PathBuf> {
    if profile_id == "default:claude" {
        let home =
            dirs::home_dir().ok_or_else(|| AppError::Io(std::io::Error::other("no home dir")))?;
        return Ok(home.join(".claude"));
    }
    let profile_root = profile_data_dir(profile_id)?;
    Ok(profile_root.join("cli-config"))
}

#[cfg(test)]
mod usage_routing_tests {
    use super::*;

    #[test]
    fn resolve_cli_config_dir_for_default_points_at_home_dot_claude() {
        let resolved = resolve_cli_config_dir("default:claude").expect("home resolvable");
        assert!(resolved.ends_with(".claude"));
        let parent = resolved.parent().expect("has parent");
        assert_eq!(parent, dirs::home_dir().unwrap().as_path());
    }

    #[test]
    fn resolve_cli_config_dir_for_managed_id_is_per_profile() {
        let resolved = resolve_cli_config_dir("some-managed-id").expect("ok");
        assert!(resolved.ends_with("cli-config"));
    }
}
