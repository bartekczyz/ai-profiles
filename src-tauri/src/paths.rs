use std::path::PathBuf;

use crate::error::{AppError, AppResult};

const APP_DIR_NAME: &str = "claude-profiles";

pub fn app_data_dir() -> AppResult<PathBuf> {
    let base = dirs::data_dir().ok_or_else(|| {
        AppError::NotFound("could not determine macOS Application Support directory".to_string())
    })?;
    Ok(base.join(APP_DIR_NAME))
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

pub fn gui_launcher_path(name: &str) -> PathBuf {
    applications_dir().join(format!("Claude ({name}).app"))
}

pub fn local_bin_dir() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::NotFound("could not determine user home directory".to_string()))?;
    Ok(home.join(".local").join("bin"))
}

pub fn cli_wrapper_path(slug: &str) -> AppResult<PathBuf> {
    Ok(local_bin_dir()?.join(format!("claude-{slug}")))
}

pub fn cli_config_dir(id: &str) -> AppResult<PathBuf> {
    Ok(profile_dir(id)?.join("cli-config"))
}

pub fn claude_desktop_install_path() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::NotFound("could not determine user home directory".to_string()))?;
    Ok(home
        .join("Library")
        .join("Application Support")
        .join("Claude"))
}

pub fn claude_code_install_path() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::NotFound("could not determine user home directory".to_string()))?;
    Ok(home.join(".claude"))
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

    #[test]
    fn gui_launcher_path_builds_expected_filename() {
        let path = gui_launcher_path("Personal");
        assert_eq!(path, PathBuf::from("/Applications/Claude (Personal).app"));
    }

    #[test]
    fn gui_launcher_path_handles_names_with_special_chars() {
        let path = gui_launcher_path("Acme & Co");
        assert_eq!(path, PathBuf::from("/Applications/Claude (Acme & Co).app"));
    }

    #[test]
    fn local_bin_dir_lives_under_home() {
        let path = local_bin_dir().unwrap();
        let home = dirs::home_dir().unwrap();
        assert_eq!(path, home.join(".local").join("bin"));
    }

    #[test]
    fn cli_wrapper_path_uses_claude_prefix() {
        let path = cli_wrapper_path("personal").unwrap();
        assert!(path.ends_with("claude-personal"));
        assert!(path.parent().unwrap().ends_with(".local/bin"));
    }

    #[test]
    fn cli_config_dir_lives_under_profile_dir() {
        let id = "11111111-1111-1111-1111-111111111111";
        let path = cli_config_dir(id).unwrap();
        assert!(path.ends_with(format!("profiles/{id}/cli-config")));
    }

    #[test]
    fn claude_desktop_install_path_lives_under_home_library() {
        let path = claude_desktop_install_path().unwrap();
        assert!(path.ends_with("Library/Application Support/Claude"));
    }

    #[test]
    fn claude_code_install_path_is_dot_claude_under_home() {
        let path = claude_code_install_path().unwrap();
        assert!(path.ends_with(".claude"));
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
