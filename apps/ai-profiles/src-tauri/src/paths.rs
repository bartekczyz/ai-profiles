use std::path::PathBuf;

use crate::app_kind::AppSpec;
use crate::error::{AppError, AppResult};

const APP_DIR_NAME: &str = "ai-profiles";

/// Returns the on-disk data directory for the app.
///
/// **Production** (`cargo build`, `cargo run`, `pnpm tauri dev`):
/// `~/Library/Application Support/ai-profiles/`.
///
/// **Tests** (`cargo test`): a per-process tempdir created lazily. The
/// tempdir lives for the duration of the test process and is cleaned up
/// automatically when the process exits.
///
/// This split exists because earlier versions of the test harness called
/// `remove_dir_all(app_data_dir())` to reset state between tests — which
/// wiped the *real* user data every time `cargo test` ran. Routing tests
/// to a tempdir makes that class of bug structurally impossible.
pub fn app_data_dir() -> AppResult<PathBuf> {
    #[cfg(test)]
    {
        Ok(test_app_data_dir())
    }
    #[cfg(not(test))]
    {
        let base = dirs::data_dir().ok_or_else(|| {
            AppError::NotFound(
                "could not determine macOS Application Support directory".to_string(),
            )
        })?;
        Ok(base.join(APP_DIR_NAME))
    }
}

#[cfg(test)]
fn test_app_data_dir() -> PathBuf {
    use std::sync::OnceLock;
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    DIR.get_or_init(|| tempfile::tempdir().expect("could not create test tempdir"))
        .path()
        .join(APP_DIR_NAME)
}

pub fn profiles_json_path() -> AppResult<PathBuf> {
    Ok(app_data_dir()?.join("profiles.json"))
}

pub fn app_state_json_path() -> AppResult<PathBuf> {
    Ok(app_data_dir()?.join("state.json"))
}

pub fn profile_dir(id: &str) -> AppResult<PathBuf> {
    Ok(app_data_dir()?.join("profiles").join(id))
}

pub fn ensure_app_dir() -> AppResult<()> {
    let dir = app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(())
}

pub fn applications_dir() -> PathBuf {
    PathBuf::from("/Applications")
}

pub fn gui_launcher_path(name: &str, spec: &AppSpec) -> PathBuf {
    applications_dir().join(format!("{} ({name}).app", spec.launcher_prefix))
}

/// Path to the stock (unmanaged) desktop application bundle. This is the app
/// the synthetic "default" entry launches — distinct from `stock_gui_support_dir`,
/// which points at the app's *data* directory under Application Support.
pub fn gui_app_bundle(spec: &AppSpec) -> PathBuf {
    applications_dir().join(spec.gui_bundle_name)
}

pub fn local_bin_dir() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::NotFound("could not determine user home directory".to_string()))?;
    Ok(home.join(".local").join("bin"))
}

pub fn cli_wrapper_path(slug: &str, spec: &AppSpec) -> AppResult<PathBuf> {
    Ok(local_bin_dir()?.join(format!("{}-{slug}", spec.cli_wrapper_prefix)))
}

pub fn cli_config_dir(id: &str) -> AppResult<PathBuf> {
    Ok(profile_dir(id)?.join("cli-config"))
}

pub fn activity_log_path(id: &str) -> AppResult<PathBuf> {
    Ok(profile_dir(id)?.join("activity.jsonl"))
}

pub fn stock_gui_support_dir(spec: &AppSpec) -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::NotFound("could not determine user home directory".to_string()))?;
    Ok(home
        .join("Library")
        .join("Application Support")
        .join(spec.gui_support_dir_name))
}

pub fn stock_cli_config_dir(spec: &AppSpec) -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::NotFound("could not determine user home directory".to_string()))?;
    Ok(home.join(spec.cli_stock_config_dir_name))
}

#[allow(dead_code)]
pub fn migration_backup_root() -> AppResult<PathBuf> {
    Ok(app_data_dir()?.join("migration-backup"))
}

pub fn next_migration_backup_dir() -> AppResult<PathBuf> {
    let stamp = chrono::Utc::now().timestamp_millis();
    Ok(app_data_dir()?.join(format!("migration-backup-{stamp}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_kind::{CLAUDE, CODEX};

    #[test]
    fn gui_launcher_path_uses_launcher_prefix() {
        assert_eq!(
            gui_launcher_path("Personal", &CLAUDE),
            PathBuf::from("/Applications/Claude (Personal).app")
        );
        assert_eq!(
            gui_launcher_path("Personal", &CODEX),
            PathBuf::from("/Applications/Codex (Personal).app")
        );
    }

    #[test]
    fn gui_app_bundle_points_to_applications_bundle() {
        assert_eq!(
            gui_app_bundle(&CLAUDE),
            PathBuf::from("/Applications/Claude.app")
        );
        assert_eq!(
            gui_app_bundle(&CODEX),
            PathBuf::from("/Applications/Codex.app")
        );
    }

    #[test]
    fn local_bin_dir_lives_under_home() {
        let path = local_bin_dir().unwrap();
        let home = dirs::home_dir().unwrap();
        assert_eq!(path, home.join(".local").join("bin"));
    }

    #[test]
    fn cli_wrapper_path_uses_wrapper_prefix() {
        assert!(cli_wrapper_path("personal", &CLAUDE)
            .unwrap()
            .ends_with("claude-personal"));
        assert!(cli_wrapper_path("personal", &CODEX)
            .unwrap()
            .ends_with("codex-personal"));
    }

    #[test]
    fn cli_config_dir_lives_under_profile_dir() {
        let id = "11111111-1111-1111-1111-111111111111";
        let path = cli_config_dir(id).unwrap();
        assert!(path.ends_with(format!("profiles/{id}/cli-config")));
    }

    #[test]
    fn stock_gui_support_dir_lives_under_home_library() {
        assert!(stock_gui_support_dir(&CODEX)
            .unwrap()
            .ends_with("Library/Application Support/Codex"));
    }

    #[test]
    fn stock_cli_config_dir_uses_spec_name() {
        assert!(stock_cli_config_dir(&CLAUDE).unwrap().ends_with(".claude"));
        assert!(stock_cli_config_dir(&CODEX).unwrap().ends_with(".codex"));
    }

    #[test]
    fn app_state_json_path_sits_next_to_profiles_json() {
        let state_path = app_state_json_path().unwrap();
        let profiles_path = profiles_json_path().unwrap();
        assert_eq!(state_path.parent(), profiles_path.parent());
        assert!(state_path.ends_with("state.json"));
    }

    #[test]
    fn next_migration_backup_dir_starts_with_prefix() {
        let path = next_migration_backup_dir().unwrap();
        let last = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(last.starts_with("migration-backup-"));
        let suffix = last.trim_start_matches("migration-backup-");
        assert!(suffix.chars().all(|character| character.is_ascii_digit()));
    }
}
