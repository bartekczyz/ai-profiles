//! Resolving profile ids to the [`Home`]s their sessions live in.

use std::path::PathBuf;

use super::Home;
use crate::app_kind::{default_id, AppKind};
use crate::app_state;
use crate::error::{AppError, AppResult};
use crate::profiles::{self, Profile};

/// What the stock install is called until the user names it.
const STOCK_LABEL: &str = "Default";

/// The home of profile `id`, or of an app's stock install for `default:<app>`.
pub fn home_for(id: &str) -> AppResult<Home> {
    if let Some(kind) = AppKind::from_default_id(id) {
        return stock_home(kind);
    }
    let profile = profiles::load()?
        .into_iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| AppError::NotFound(format!("profile {id} not found")))?;
    managed_home(&profile)
}

/// Every home of `kind`: the stock install first, then the managed profiles in
/// their saved order.
pub fn homes_of(kind: AppKind) -> AppResult<Vec<Home>> {
    let mut homes = vec![stock_home(kind)?];
    for profile in profiles::load()?
        .iter()
        .filter(|profile| profile.app == kind)
    {
        homes.push(managed_home(profile)?);
    }
    Ok(homes)
}

/// The home of `kind`'s stock install, labelled with the name the user gave
/// its entry, else [`STOCK_LABEL`]. The name is only a label, so a state file
/// that can't be read leaves the stock one in place rather than failing.
fn stock_home(kind: AppKind) -> AppResult<Home> {
    let id = default_id(kind);
    let paths = profiles::paths(&id)?;
    let label = app_state::load()
        .ok()
        .and_then(|state| state.default_profile_names.get(&kind).cloned())
        .unwrap_or_else(|| STOCK_LABEL.to_string());
    Ok(Home {
        id,
        app: kind,
        label,
        config_dir: PathBuf::from(paths.cli_config_dir),
        gui_data_dir: PathBuf::from(paths.gui_data_dir),
        stock: true,
    })
}

/// The home of a managed `profile`.
fn managed_home(profile: &Profile) -> AppResult<Home> {
    let paths = profiles::paths(&profile.id)?;
    Ok(Home {
        id: profile.id.clone(),
        app: profile.app,
        label: profile.name.clone(),
        config_dir: PathBuf::from(paths.cli_config_dir),
        gui_data_dir: PathBuf::from(paths.gui_data_dir),
        stock: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::{AppStatePatch, DefaultProfileName};
    use crate::paths::{profile_dir, stock_cli_config_dir, stock_gui_support_dir};
    use crate::profiles::Surfaces;
    use crate::test_support::APP_DIR_TEST_LOCK;

    fn purge() {
        let _ = std::fs::remove_dir_all(crate::paths::app_data_dir().unwrap());
    }

    fn profile(id: &str, app: AppKind, name: &str) -> Profile {
        Profile {
            id: id.to_string(),
            app,
            name: name.to_string(),
            slug: name.to_lowercase(),
            color: "#123456".to_string(),
            created_at: "2026-09-23T00:00:00Z".to_string(),
            surfaces: Surfaces {
                gui: true,
                cli: true,
            },
            distinct_dock_icon: false,
            last_used_at: None,
        }
    }

    #[test]
    fn the_default_id_resolves_to_the_stock_install() {
        let _guard = APP_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        purge();

        let home = home_for("default:claude").unwrap();

        let spec = AppKind::Claude.spec();
        assert_eq!(home.id, "default:claude");
        assert_eq!(home.app, AppKind::Claude);
        assert_eq!(home.label, "Default");
        assert!(home.stock);
        assert_eq!(home.config_dir, stock_cli_config_dir(spec).unwrap());
        assert_eq!(home.gui_data_dir, stock_gui_support_dir(spec).unwrap());
    }

    #[test]
    fn the_stock_install_is_labelled_with_the_name_the_user_gave_it() {
        let _guard = APP_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        purge();
        app_state::apply(AppStatePatch {
            default_profile_name: Some(DefaultProfileName {
                app: AppKind::Codex,
                name: "Personal".to_string(),
            }),
            ..AppStatePatch::default()
        })
        .unwrap();

        let label = home_for("default:codex").unwrap().label;
        purge();

        assert_eq!(label, "Personal");
    }

    #[test]
    fn homes_of_an_app_are_its_stock_install_then_its_profiles() {
        let _guard = APP_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        purge();
        profiles::save_all(&[
            profile("work", AppKind::Claude, "Work"),
            profile("chat", AppKind::Codex, "Chat"),
            profile("side", AppKind::Claude, "Side"),
        ])
        .unwrap();

        let homes = homes_of(AppKind::Claude).unwrap();
        let unknown = home_for("missing");
        purge();

        let ids: Vec<&str> = homes.iter().map(|home| home.id.as_str()).collect();
        assert_eq!(ids, ["default:claude", "work", "side"]);
        let work = &homes[1];
        assert_eq!(work.label, "Work");
        assert!(!work.stock);
        assert_eq!(
            work.config_dir,
            profile_dir("work").unwrap().join("cli-config")
        );
        assert_eq!(
            work.gui_data_dir,
            profile_dir("work").unwrap().join("gui-data")
        );
        assert!(matches!(unknown, Err(AppError::NotFound(_))));
    }
}
