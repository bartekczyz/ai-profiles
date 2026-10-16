//! Persisted app-level state — independent of profiles.json.
//! Holds first-run flags and dismissal timestamps.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::app_kind::AppKind;
use crate::error::{AppError, AppResult};
use crate::paths::{app_state_json_path, ensure_app_dir};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    #[default]
    System,
    Dark,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    #[serde(default)]
    pub welcome_shown: bool,
    #[serde(default)]
    pub migration_dismissed_at: Option<String>,
    #[serde(default)]
    pub path_banner_dismissed_at: Option<String>,
    #[serde(default)]
    pub theme_mode: ThemeMode,
    #[serde(default)]
    pub selected_entry_id: Option<String>,
    /// When the user first said they understood what giving a profile its own
    /// Dock icon involves (a re-signed copy of the app, a fresh sign-in, no
    /// Gatekeeper on the copy). Until then the app explains it before the first
    /// profile gets one; afterwards it does not ask again.
    #[serde(default)]
    pub dock_icon_acknowledged_at: Option<String>,
    /// Display names the user gave the stock-install ("Default") entries, by
    /// app. Label only: nothing on disk is named after it. Absent means the
    /// entry shows its stock label.
    #[serde(default)]
    pub default_profile_names: BTreeMap<AppKind, String>,
    /// The sessions the user said not now to repairing, by profile id: the
    /// ones that needed repair when they dismissed the offer. The offer
    /// stays away until a session not among them needs repair.
    #[serde(default)]
    pub dismissed_repair_sessions: BTreeMap<String, Vec<String>>,
}

/// Renames one app's stock-install entry. An empty (or all-whitespace) name
/// clears the custom name, putting the stock label back.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultProfileName {
    pub app: AppKind,
    pub name: String,
}

const DEFAULT_PROFILE_NAME_MAX_CHARS: usize = 64;

/// Sets the sessions a profile's repair offer was dismissed for. An empty
/// list forgets the dismissal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DismissedRepair {
    pub profile_id: String,
    pub session_ids: Vec<String>,
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
    pub theme_mode: Option<ThemeMode>,
    #[serde(default)]
    pub clear_migration_dismissed: bool,
    #[serde(default)]
    pub clear_path_banner_dismissed: bool,
    #[serde(default)]
    pub selected_entry_id: Option<String>,
    #[serde(default)]
    pub clear_selected_entry_id: bool,
    /// Sets the acknowledgement. There is no way to take it back: it records
    /// something the user has been told.
    #[serde(default)]
    pub dock_icon_acknowledged_at: Option<String>,
    #[serde(default)]
    pub default_profile_name: Option<DefaultProfileName>,
    #[serde(default)]
    pub dismissed_repair: Option<DismissedRepair>,
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
    if let Some(theme) = patch.theme_mode {
        state.theme_mode = theme;
    }
    if patch.clear_selected_entry_id {
        state.selected_entry_id = None;
    } else if patch.selected_entry_id.is_some() {
        state.selected_entry_id = patch.selected_entry_id;
    }
    if patch.dock_icon_acknowledged_at.is_some() {
        state.dock_icon_acknowledged_at = patch.dock_icon_acknowledged_at;
    }
    if let Some(rename) = patch.default_profile_name {
        let name = rename.name.trim();
        if name.is_empty() {
            state.default_profile_names.remove(&rename.app);
        } else {
            validate_default_profile_name(name)?;
            state
                .default_profile_names
                .insert(rename.app, name.to_string());
        }
    }
    if let Some(dismissed) = patch.dismissed_repair {
        if dismissed.session_ids.is_empty() {
            state
                .dismissed_repair_sessions
                .remove(&dismissed.profile_id);
        } else {
            state
                .dismissed_repair_sessions
                .insert(dismissed.profile_id, dismissed.session_ids);
        }
    }
    save(&state)?;
    Ok(state)
}

/// Drop what the state keeps about profile `id`, which is gone. Writes
/// nothing when it keeps nothing.
pub fn forget_profile(id: &str) -> AppResult<()> {
    let mut state = load()?;
    if state.dismissed_repair_sessions.remove(id).is_none() {
        return Ok(());
    }
    save(&state)
}

fn validate_default_profile_name(name: &str) -> AppResult<()> {
    if name.chars().any(char::is_control) {
        return Err(AppError::Validation(
            "name must not contain control characters".to_string(),
        ));
    }
    if name.chars().count() > DEFAULT_PROFILE_NAME_MAX_CHARS {
        return Err(AppError::Validation(format!(
            "name must be at most {DEFAULT_PROFILE_NAME_MAX_CHARS} characters"
        )));
    }
    Ok(())
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
    use crate::test_support::APP_DIR_TEST_LOCK as TEST_LOCK;

    fn purge() {
        let _ = std::fs::remove_dir_all(crate::paths::app_data_dir().unwrap());
    }

    #[test]
    fn default_state_has_all_defaults() {
        let state = AppState::default();
        assert!(!state.welcome_shown);
        assert_eq!(state.migration_dismissed_at, None);
        assert_eq!(state.path_banner_dismissed_at, None);
        assert_eq!(state.theme_mode, ThemeMode::System);
    }

    #[test]
    fn apply_persists_theme_mode() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge();
        let after = apply(AppStatePatch {
            theme_mode: Some(ThemeMode::Dark),
            ..AppStatePatch::default()
        })
        .unwrap();
        assert_eq!(after.theme_mode, ThemeMode::Dark);
        let reloaded = load().unwrap();
        assert_eq!(reloaded.theme_mode, ThemeMode::Dark);
        purge();
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
            theme_mode: ThemeMode::default(),
            selected_entry_id: None,
            dock_icon_acknowledged_at: Some("2026-05-21T09:30:00Z".into()),
            default_profile_names: BTreeMap::from([(AppKind::Claude, "Personal".into())]),
            dismissed_repair_sessions: BTreeMap::from([("p1".into(), vec!["s1".into()])]),
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
            theme_mode: ThemeMode::default(),
            selected_entry_id: None,
            dock_icon_acknowledged_at: Some("acknowledged".into()),
            default_profile_names: BTreeMap::new(),
            dismissed_repair_sessions: BTreeMap::new(),
        })
        .unwrap();

        let after = apply(AppStatePatch {
            welcome_shown: Some(true),
            ..AppStatePatch::default()
        })
        .unwrap();
        assert!(after.welcome_shown);
        assert_eq!(after.migration_dismissed_at.as_deref(), Some("old"));
        assert_eq!(
            after.dock_icon_acknowledged_at.as_deref(),
            Some("acknowledged")
        );
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
            theme_mode: ThemeMode::default(),
            selected_entry_id: None,
            dock_icon_acknowledged_at: None,
            default_profile_names: BTreeMap::new(),
            dismissed_repair_sessions: BTreeMap::new(),
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

    #[test]
    fn default_state_selected_entry_id_is_none() {
        let state = AppState::default();
        assert_eq!(state.selected_entry_id, None);
    }

    #[test]
    fn apply_persists_selected_entry_id() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge();
        let after = apply(AppStatePatch {
            selected_entry_id: Some("profile-abc".into()),
            ..AppStatePatch::default()
        })
        .unwrap();
        assert_eq!(after.selected_entry_id.as_deref(), Some("profile-abc"));
        let reloaded = load().unwrap();
        assert_eq!(reloaded.selected_entry_id.as_deref(), Some("profile-abc"));
        purge();
    }

    #[test]
    fn apply_can_clear_selected_entry_id() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge();
        save(&AppState {
            welcome_shown: true,
            migration_dismissed_at: None,
            path_banner_dismissed_at: None,
            theme_mode: ThemeMode::default(),
            selected_entry_id: Some("profile-xyz".into()),
            dock_icon_acknowledged_at: None,
            default_profile_names: BTreeMap::new(),
            dismissed_repair_sessions: BTreeMap::new(),
        })
        .unwrap();
        let after = apply(AppStatePatch {
            clear_selected_entry_id: true,
            ..AppStatePatch::default()
        })
        .unwrap();
        assert_eq!(after.selected_entry_id, None);
        purge();
    }

    #[test]
    fn the_dock_icon_acknowledgement_starts_unset_and_sticks_once_given() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge();
        assert_eq!(AppState::default().dock_icon_acknowledged_at, None);

        let after = apply(AppStatePatch {
            dock_icon_acknowledged_at: Some("2026-05-21T09:30:00Z".into()),
            ..AppStatePatch::default()
        })
        .unwrap();
        assert_eq!(
            after.dock_icon_acknowledged_at.as_deref(),
            Some("2026-05-21T09:30:00Z")
        );

        // A patch that says nothing about it, or every clear flag there is,
        // leaves it alone: it records something the user was told.
        let untouched = apply(AppStatePatch {
            welcome_shown: Some(true),
            clear_migration_dismissed: true,
            clear_path_banner_dismissed: true,
            clear_selected_entry_id: true,
            ..AppStatePatch::default()
        })
        .unwrap();
        assert_eq!(
            untouched.dock_icon_acknowledged_at.as_deref(),
            Some("2026-05-21T09:30:00Z")
        );
        assert_eq!(
            load().unwrap().dock_icon_acknowledged_at.as_deref(),
            Some("2026-05-21T09:30:00Z")
        );
        purge();
    }

    #[test]
    fn state_saved_before_the_acknowledgement_existed_loads_as_not_acknowledged() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge();
        crate::paths::ensure_app_dir().unwrap();
        std::fs::write(
            crate::paths::app_state_json_path().unwrap(),
            r#"{"welcomeShown": true, "themeMode": "dark"}"#,
        )
        .unwrap();
        assert_eq!(load().unwrap().dock_icon_acknowledged_at, None);
        purge();
    }

    fn rename_default(app: AppKind, name: &str) -> AppResult<AppState> {
        apply(AppStatePatch {
            default_profile_name: Some(DefaultProfileName {
                app,
                name: name.to_string(),
            }),
            ..AppStatePatch::default()
        })
    }

    #[test]
    fn apply_sets_trims_and_clears_default_profile_names() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge();
        rename_default(AppKind::Claude, "  Personal  ").unwrap();
        let after = rename_default(AppKind::Codex, "Work").unwrap();
        assert_eq!(
            after
                .default_profile_names
                .get(&AppKind::Claude)
                .map(String::as_str),
            Some("Personal")
        );
        assert_eq!(
            load()
                .unwrap()
                .default_profile_names
                .get(&AppKind::Codex)
                .map(String::as_str),
            Some("Work")
        );

        let cleared = rename_default(AppKind::Claude, "   ").unwrap();
        assert!(!cleared.default_profile_names.contains_key(&AppKind::Claude));
        assert!(cleared.default_profile_names.contains_key(&AppKind::Codex));
        purge();
    }

    #[test]
    fn apply_rejects_bad_default_profile_names_without_saving() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge();
        assert!(rename_default(AppKind::Claude, "Work\nrm -rf ~").is_err());
        assert!(rename_default(AppKind::Claude, &"x".repeat(65)).is_err());
        assert!(load().unwrap().default_profile_names.is_empty());
        purge();
    }

    fn dismiss(profile_id: &str, session_ids: &[&str]) -> AppResult<AppState> {
        apply(AppStatePatch {
            dismissed_repair: Some(DismissedRepair {
                profile_id: profile_id.into(),
                session_ids: session_ids.iter().map(|id| (*id).to_string()).collect(),
            }),
            ..AppStatePatch::default()
        })
    }

    #[test]
    fn a_dismissed_repair_is_kept_per_profile_and_forgotten_when_emptied() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge();
        dismiss("p1", &["a", "b"]).unwrap();
        dismiss("p2", &["c"]).unwrap();
        assert_eq!(
            load().unwrap().dismissed_repair_sessions.get("p1"),
            Some(&vec!["a".to_string(), "b".to_string()])
        );

        let after = dismiss("p1", &[]).unwrap();
        assert!(!after.dismissed_repair_sessions.contains_key("p1"));
        assert!(after.dismissed_repair_sessions.contains_key("p2"));
        purge();
    }

    #[test]
    fn forgetting_a_profile_drops_its_dismissed_repair() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge();
        dismiss("p1", &["a"]).unwrap();
        dismiss("p2", &["c"]).unwrap();
        forget_profile("p1").unwrap();
        let state = load().unwrap();
        assert!(!state.dismissed_repair_sessions.contains_key("p1"));
        assert!(state.dismissed_repair_sessions.contains_key("p2"));
        purge();
    }

    #[test]
    fn state_without_default_profile_names_still_loads() {
        let parsed: AppState = serde_json::from_str(r#"{"welcomeShown": true}"#).unwrap();
        assert!(parsed.default_profile_names.is_empty());
    }
}
