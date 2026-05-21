//! Persisted app-level state — independent of profiles.json.
//! Holds first-run flags and dismissal timestamps.

use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::paths::{app_state_json_path, ensure_app_dir};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    #[serde(default)]
    pub welcome_shown: bool,
    #[serde(default)]
    pub migration_dismissed_at: Option<String>,
    #[serde(default)]
    pub path_banner_dismissed_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatePatch {
    #[serde(default)]
    pub welcome_shown: Option<bool>,
    #[serde(default)]
    pub migration_dismissed_at: Option<String>,
    #[serde(default)]
    pub path_banner_dismissed_at: Option<String>,
    #[serde(default)]
    pub clear_migration_dismissed: bool,
    #[serde(default)]
    pub clear_path_banner_dismissed: bool,
}

pub fn load() -> AppResult<AppState> {
    let path = app_state_json_path()?;
    if !path.exists() {
        return Ok(AppState::default());
    }
    let raw = fs::read_to_string(&path)?;
    if raw.trim().is_empty() {
        return Ok(AppState::default());
    }
    let state: AppState = serde_json::from_str(&raw)?;
    Ok(state)
}

pub fn save(state: &AppState) -> AppResult<()> {
    ensure_app_dir()?;
    let path = app_state_json_path()?;
    let body = serde_json::to_vec_pretty(state)?;
    atomic_write(&path, &body)?;
    Ok(())
}

pub fn apply(patch: AppStatePatch) -> AppResult<AppState> {
    let mut state = load()?;
    if let Some(welcome) = patch.welcome_shown {
        state.welcome_shown = welcome;
    }
    if patch.clear_migration_dismissed {
        state.migration_dismissed_at = None;
    } else if patch.migration_dismissed_at.is_some() {
        state.migration_dismissed_at = patch.migration_dismissed_at;
    }
    if patch.clear_path_banner_dismissed {
        state.path_banner_dismissed_at = None;
    } else if patch.path_banner_dismissed_at.is_some() {
        state.path_banner_dismissed_at = patch.path_banner_dismissed_at;
    }
    save(&state)?;
    Ok(state)
}

fn atomic_write(path: &Path, body: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::NotFound(format!("path {} has no parent", path.display())))?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(
        path.file_name()
            .map(|name| format!(".{}.tmp", name.to_string_lossy()))
            .unwrap_or_else(|| ".tmp".to_string()),
    );
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(body)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn purge() {
        let _ = std::fs::remove_dir_all(crate::paths::app_data_dir().unwrap());
    }

    #[test]
    fn default_state_has_all_defaults() {
        let state = AppState::default();
        assert!(!state.welcome_shown);
        assert_eq!(state.migration_dismissed_at, None);
        assert_eq!(state.path_banner_dismissed_at, None);
    }

    #[test]
    fn load_returns_default_when_file_missing() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge();
        let state = load().unwrap();
        assert_eq!(state, AppState::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge();
        let state = AppState {
            welcome_shown: true,
            migration_dismissed_at: Some("2026-05-20T12:00:00Z".into()),
            path_banner_dismissed_at: None,
        };
        save(&state).unwrap();
        let loaded = load().unwrap();
        assert_eq!(loaded, state);
        purge();
    }

    #[test]
    fn apply_patches_only_specified_fields() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge();
        save(&AppState {
            welcome_shown: false,
            migration_dismissed_at: Some("old".into()),
            path_banner_dismissed_at: None,
        })
        .unwrap();

        let after = apply(AppStatePatch {
            welcome_shown: Some(true),
            ..AppStatePatch::default()
        })
        .unwrap();
        assert!(after.welcome_shown);
        assert_eq!(after.migration_dismissed_at.as_deref(), Some("old"));
        purge();
    }

    #[test]
    fn apply_can_clear_dismissal_timestamps() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge();
        save(&AppState {
            welcome_shown: true,
            migration_dismissed_at: Some("set".into()),
            path_banner_dismissed_at: Some("set".into()),
        })
        .unwrap();

        let after = apply(AppStatePatch {
            clear_migration_dismissed: true,
            clear_path_banner_dismissed: true,
            ..AppStatePatch::default()
        })
        .unwrap();
        assert_eq!(after.migration_dismissed_at, None);
        assert_eq!(after.path_banner_dismissed_at, None);
        purge();
    }

    #[test]
    fn unknown_fields_in_json_are_ignored_on_load() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge();
        crate::paths::ensure_app_dir().unwrap();
        std::fs::write(
            crate::paths::app_state_json_path().unwrap(),
            r#"{"welcomeShown": true, "someFutureField": 42}"#,
        )
        .unwrap();
        let loaded = load().unwrap();
        assert!(loaded.welcome_shown);
        purge();
    }
}
