use std::process::Command;

use crate::error::{AppError, AppResult};
use crate::paths::gui_launcher_path;
use crate::profiles::{self, Profile, ProfilePatch, ProfilePaths, Surface, Surfaces};

#[tauri::command]
pub fn list_profiles() -> AppResult<Vec<Profile>> {
    profiles::load()
}

#[tauri::command]
pub fn create_profile(name: String, color: String, surfaces: Surfaces) -> AppResult<Profile> {
    profiles::create(&name, &color, surfaces)
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
    profiles::update(&id, patch)
}

#[tauri::command]
pub fn delete_profile(id: String, move_to_trash: bool) -> AppResult<()> {
    profiles::delete(&id, move_to_trash)
}

#[tauri::command]
pub fn toggle_surface(id: String, surface: Surface, enabled: bool) -> AppResult<Profile> {
    profiles::toggle_surface(&id, surface, enabled)
}

#[tauri::command]
pub fn open_profile_in_app(id: String) -> AppResult<()> {
    let all = profiles::load()?;
    let profile = all
        .iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| AppError::NotFound(format!("profile {id} not found")))?;
    if !profile.surfaces.gui {
        return Err(AppError::Validation("profile has no GUI surface".into()));
    }
    let app_path = gui_launcher_path(&profile.name);
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

#[tauri::command]
pub fn profile_paths(id: String) -> AppResult<ProfilePaths> {
    profiles::paths(&id)
}
