use std::fs;
use std::io::Write;
use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::paths::{ensure_app_dir, profile_dir, profiles_json_path};
use crate::slug::slugify;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Surfaces {
    pub gui: bool,
    pub cli: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub color: String,
    pub created_at: String,
    pub surfaces: Surfaces,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilePatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Surface {
    Gui,
    Cli,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilePaths {
    pub data_dir: String,
    pub gui_data_dir: String,
    pub cli_config_dir: String,
    pub gui_launcher_path: String,
    pub cli_wrapper_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Store {
    profiles: Vec<Profile>,
}

pub fn load() -> AppResult<Vec<Profile>> {
    let path = profiles_json_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&path)?;
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    let store: Store = serde_json::from_str(&raw)?;
    Ok(store.profiles)
}

pub fn save_all(profiles: &[Profile]) -> AppResult<()> {
    ensure_app_dir()?;
    let path = profiles_json_path()?;
    let store = Store {
        profiles: profiles.to_vec(),
    };
    let body = serde_json::to_vec_pretty(&store)?;
    atomic_write(&path, &body)?;
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

pub fn create(name: &str, color: &str, surfaces: Surfaces) -> AppResult<Profile> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("name must not be empty".to_string()));
    }
    if !is_valid_hex_color(color) {
        return Err(AppError::Validation(format!(
            "color must be a 7-char hex like #7C3AED, got '{color}'"
        )));
    }
    let slug = slugify(trimmed);
    if slug.is_empty() {
        return Err(AppError::Validation(
            "name produced an empty slug after sanitisation".to_string(),
        ));
    }

    let mut existing = load()?;
    if existing.iter().any(|profile| profile.slug == slug) {
        return Err(AppError::Validation(format!(
            "a profile with slug '{slug}' already exists"
        )));
    }

    let profile = Profile {
        id: Uuid::new_v4().to_string(),
        name: trimmed.to_string(),
        slug,
        color: color.to_string(),
        created_at: Utc::now().to_rfc3339(),
        surfaces,
    };

    let dir = profile_dir(&profile.id)?;
    if profile.surfaces.gui {
        fs::create_dir_all(dir.join("gui-data"))?;
    }
    if profile.surfaces.cli {
        fs::create_dir_all(dir.join("cli-config"))?;
    }

    // Generate launchers BEFORE persisting, so a launcher failure rolls back cleanly.
    if profile.surfaces.gui {
        if let Err(err) = crate::launchers::gui::generate(&profile, env!("CARGO_PKG_VERSION")) {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(err);
        }
    }

    if profile.surfaces.cli {
        if let Err(err) = crate::launchers::cli::generate(&profile) {
            if profile.surfaces.gui {
                let _ = crate::launchers::gui::remove(&profile.name);
            }
            let _ = std::fs::remove_dir_all(&dir);
            return Err(err);
        }
    }

    existing.push(profile.clone());
    if let Err(err) = save_all(&existing) {
        if profile.surfaces.cli {
            let _ = crate::launchers::cli::remove(&profile.slug);
        }
        if profile.surfaces.gui {
            let _ = crate::launchers::gui::remove(&profile.name);
        }
        let _ = std::fs::remove_dir_all(&dir);
        return Err(err);
    }
    Ok(profile)
}

fn is_valid_hex_color(color: &str) -> bool {
    if color.len() != 7 || !color.starts_with('#') {
        return false;
    }
    color.chars().skip(1).all(|ch| ch.is_ascii_hexdigit())
}

pub fn update(id: &str, patch: ProfilePatch) -> AppResult<Profile> {
    let mut all = load()?;
    let position = all
        .iter()
        .position(|profile| profile.id == id)
        .ok_or_else(|| AppError::NotFound(format!("profile {id} not found")))?;
    let original = all[position].clone();

    let new_name = patch
        .name
        .as_deref()
        .unwrap_or(&original.name)
        .trim()
        .to_string();
    if new_name.is_empty() {
        return Err(AppError::Validation("name must not be empty".into()));
    }
    let new_color = patch.color.unwrap_or_else(|| original.color.clone());
    if !is_valid_hex_color(&new_color) {
        return Err(AppError::Validation(format!(
            "color must be #RRGGBB, got '{new_color}'"
        )));
    }
    let new_slug = slugify(&new_name);
    if new_slug.is_empty() {
        return Err(AppError::Validation(
            "name produced an empty slug after sanitisation".into(),
        ));
    }
    if new_slug != original.slug
        && all
            .iter()
            .any(|other| other.id != id && other.slug == new_slug)
    {
        return Err(AppError::Validation(format!(
            "a profile with slug '{new_slug}' already exists"
        )));
    }

    let updated = Profile {
        id: original.id.clone(),
        name: new_name,
        slug: new_slug,
        color: new_color,
        created_at: original.created_at.clone(),
        surfaces: original.surfaces.clone(),
    };

    if updated.surfaces.gui {
        crate::launchers::gui::generate(&updated, env!("CARGO_PKG_VERSION"))?;
    }
    if updated.surfaces.cli {
        if let Err(err) = crate::launchers::cli::generate(&updated) {
            if updated.surfaces.gui {
                let _ = crate::launchers::gui::remove(&updated.name);
            }
            return Err(err);
        }
    }

    all[position] = updated.clone();
    if let Err(err) = save_all(&all) {
        if updated.surfaces.cli {
            let _ = crate::launchers::cli::remove(&updated.slug);
        }
        if updated.surfaces.gui {
            let _ = crate::launchers::gui::remove(&updated.name);
        }
        return Err(err);
    }

    if updated.name != original.name && original.surfaces.gui {
        let _ = crate::launchers::gui::remove(&original.name);
    }
    if updated.slug != original.slug && original.surfaces.cli {
        let _ = crate::launchers::cli::remove(&original.slug);
    }

    Ok(updated)
}

pub fn delete(id: &str, move_to_trash: bool) -> AppResult<()> {
    let mut all = load()?;
    let position = all
        .iter()
        .position(|profile| profile.id == id)
        .ok_or_else(|| AppError::NotFound(format!("profile {id} not found")))?;
    let profile = all[position].clone();

    if profile.surfaces.gui {
        let _ = crate::launchers::gui::remove(&profile.name);
    }
    if profile.surfaces.cli {
        let _ = crate::launchers::cli::remove(&profile.slug);
    }

    let dir = crate::paths::profile_dir(&profile.id)?;
    if dir.exists() {
        if move_to_trash {
            trash::delete(&dir).map_err(|err| {
                AppError::Validation(format!("failed to move {} to Trash: {err}", dir.display()))
            })?;
        } else {
            std::fs::remove_dir_all(&dir)?;
        }
    }

    all.remove(position);
    save_all(&all)?;
    Ok(())
}

pub fn toggle_surface(id: &str, surface: Surface, enabled: bool) -> AppResult<Profile> {
    let mut all = load()?;
    let position = all
        .iter()
        .position(|profile| profile.id == id)
        .ok_or_else(|| AppError::NotFound(format!("profile {id} not found")))?;
    let mut profile = all[position].clone();

    let already = match surface {
        Surface::Gui => profile.surfaces.gui,
        Surface::Cli => profile.surfaces.cli,
    };
    if already == enabled {
        return Ok(profile);
    }

    let dir = crate::paths::profile_dir(&profile.id)?;
    if enabled {
        match surface {
            Surface::Gui => {
                fs::create_dir_all(dir.join("gui-data"))?;
                profile.surfaces.gui = true;
                crate::launchers::gui::generate(&profile, env!("CARGO_PKG_VERSION"))?;
            }
            Surface::Cli => {
                fs::create_dir_all(dir.join("cli-config"))?;
                profile.surfaces.cli = true;
                crate::launchers::cli::generate(&profile)?;
            }
        }
    } else {
        match surface {
            Surface::Gui => {
                let _ = crate::launchers::gui::remove(&profile.name);
                profile.surfaces.gui = false;
            }
            Surface::Cli => {
                let _ = crate::launchers::cli::remove(&profile.slug);
                profile.surfaces.cli = false;
            }
        }
    }

    all[position] = profile.clone();
    save_all(&all)?;
    Ok(profile)
}

pub fn paths(id: &str) -> AppResult<ProfilePaths> {
    let all = load()?;
    let profile = all
        .iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| AppError::NotFound(format!("profile {id} not found")))?;
    let data_dir = crate::paths::profile_dir(&profile.id)?;
    Ok(ProfilePaths {
        data_dir: data_dir.display().to_string(),
        gui_data_dir: data_dir.join("gui-data").display().to_string(),
        cli_config_dir: data_dir.join("cli-config").display().to_string(),
        gui_launcher_path: crate::paths::gui_launcher_path(&profile.name)
            .display()
            .to_string(),
        cli_wrapper_path: crate::paths::cli_wrapper_path(&profile.slug)?
            .display()
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::APP_DIR_TEST_LOCK as TEST_LOCK;

    fn purge_for_test() {
        let _ = std::fs::remove_dir_all(crate::paths::app_data_dir().unwrap());
    }

    #[test]
    fn hex_color_validator_accepts_correct_format() {
        assert!(is_valid_hex_color("#7C3AED"));
        assert!(is_valid_hex_color("#000000"));
        assert!(is_valid_hex_color("#ffffff"));
    }

    #[test]
    fn hex_color_validator_rejects_bad_input() {
        assert!(!is_valid_hex_color("7C3AED"));
        assert!(!is_valid_hex_color("#7C3AE"));
        assert!(!is_valid_hex_color("#GGGGGG"));
        assert!(!is_valid_hex_color(""));
    }

    #[test]
    fn profile_roundtrips_through_json() {
        let original = Profile {
            id: "11111111-1111-1111-1111-111111111111".to_string(),
            name: "Personal".to_string(),
            slug: "personal".to_string(),
            color: "#7C3AED".to_string(),
            created_at: "2026-05-20T12:00:00Z".to_string(),
            surfaces: Surfaces {
                gui: true,
                cli: true,
            },
        };
        let raw = serde_json::to_string(&original).unwrap();
        let parsed: Profile = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed, original);
        assert!(raw.contains(r#""createdAt""#));
    }

    #[test]
    fn update_changes_name_and_slug_atomically() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge_for_test();

        if std::env::var("CLAUDE_PROFILES_E2E").is_err() {
            eprintln!("skipping; set CLAUDE_PROFILES_E2E=1 to run");
            return;
        }

        let created = create(
            "Original",
            "#7C3AED",
            Surfaces {
                gui: false,
                cli: true,
            },
        )
        .unwrap();

        let patched = update(
            &created.id,
            ProfilePatch {
                name: Some("Renamed".into()),
                color: None,
            },
        )
        .unwrap();
        assert_eq!(patched.name, "Renamed");
        assert_eq!(patched.slug, "renamed");
        assert_eq!(patched.color, "#7C3AED");

        let old_wrapper = crate::paths::cli_wrapper_path("original").unwrap();
        let new_wrapper = crate::paths::cli_wrapper_path("renamed").unwrap();
        assert!(!old_wrapper.exists());
        assert!(new_wrapper.exists());

        delete(&patched.id, false).unwrap();
        purge_for_test();
    }

    #[test]
    fn update_rejects_slug_collision() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge_for_test();
        if std::env::var("CLAUDE_PROFILES_E2E").is_err() {
            eprintln!("skipping; set CLAUDE_PROFILES_E2E=1 to run");
            return;
        }

        let first = create(
            "Alpha",
            "#7C3AED",
            Surfaces {
                gui: false,
                cli: false,
            },
        )
        .unwrap();
        let second = create(
            "Beta",
            "#3B82F6",
            Surfaces {
                gui: false,
                cli: false,
            },
        )
        .unwrap();

        let err = update(
            &second.id,
            ProfilePatch {
                name: Some("Alpha".into()),
                color: None,
            },
        )
        .unwrap_err();
        match err {
            AppError::Validation(msg) => assert!(msg.contains("already exists")),
            other => panic!("expected Validation, got {other:?}"),
        }

        delete(&first.id, false).unwrap();
        delete(&second.id, false).unwrap();
        purge_for_test();
    }

    #[test]
    fn toggle_surface_off_keeps_data_dir() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge_for_test();
        if std::env::var("CLAUDE_PROFILES_E2E").is_err() {
            eprintln!("skipping; set CLAUDE_PROFILES_E2E=1 to run");
            return;
        }

        let profile = create(
            "ToggleTest",
            "#10B981",
            Surfaces {
                gui: false,
                cli: true,
            },
        )
        .unwrap();
        let cli_config = crate::paths::cli_config_dir(&profile.id).unwrap();
        std::fs::write(cli_config.join("session.json"), b"hello").unwrap();

        toggle_surface(&profile.id, Surface::Cli, false).unwrap();
        assert!(
            cli_config.join("session.json").exists(),
            "data dir must survive toggle-off"
        );
        assert!(!crate::paths::cli_wrapper_path(&profile.slug)
            .unwrap()
            .exists());

        delete(&profile.id, false).unwrap();
        purge_for_test();
    }

    #[test]
    fn delete_removes_profile_and_launchers() {
        let _guard = TEST_LOCK.lock().unwrap();
        purge_for_test();
        if std::env::var("CLAUDE_PROFILES_E2E").is_err() {
            eprintln!("skipping; set CLAUDE_PROFILES_E2E=1 to run");
            return;
        }

        let profile = create(
            "DeleteMe",
            "#EF4444",
            Surfaces {
                gui: false,
                cli: true,
            },
        )
        .unwrap();
        let wrapper = crate::paths::cli_wrapper_path(&profile.slug).unwrap();
        assert!(wrapper.exists());

        delete(&profile.id, false).unwrap();
        assert!(!wrapper.exists());
        assert!(load().unwrap().is_empty());
        purge_for_test();
    }
}
