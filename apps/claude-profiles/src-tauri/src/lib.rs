// SPDX-License-Identifier: MIT

mod activity;
mod app_state;
mod commands;
mod deps;
mod error;
mod launchers;
mod migration;
mod path_setup;
mod paths;
mod profiles;
mod slug;
#[cfg(test)]
mod test_support;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::Emitter;

/// Custom menu identifier for the App menu's "About claude-profiles" entry.
/// The default macOS About item opens a tiny system panel; we replace it
/// with an item that emits an event the frontend listens for, so the
/// click opens our own Atelier-styled `<AboutDialog>` instead.
const OPEN_ABOUT_ID: &str = "open-about";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            // Build the macOS app menu manually so we can swap the default
            // About panel for a frontend-driven dialog. Everything else here
            // mirrors what Tauri would auto-generate (Services, Hide,
            // Hide Others, Show All, Quit) so the menu stays familiar.
            let about = MenuItem::with_id(
                app,
                OPEN_ABOUT_ID,
                "About claude-profiles",
                true,
                None::<&str>,
            )?;
            let app_submenu = Submenu::with_items(
                app,
                "claude-profiles",
                true,
                &[
                    &about,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::services(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::hide(app, None)?,
                    &PredefinedMenuItem::hide_others(app, None)?,
                    &PredefinedMenuItem::show_all(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::quit(app, None)?,
                ],
            )?;
            // Edit submenu so Cmd+C / Cmd+V / Cmd+Z keep working in dialog
            // inputs — these would otherwise be silently dropped because
            // setting a custom menu replaces the default one entirely.
            let edit_submenu = Submenu::with_items(
                app,
                "Edit",
                true,
                &[
                    &PredefinedMenuItem::undo(app, None)?,
                    &PredefinedMenuItem::redo(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::cut(app, None)?,
                    &PredefinedMenuItem::copy(app, None)?,
                    &PredefinedMenuItem::paste(app, None)?,
                    &PredefinedMenuItem::select_all(app, None)?,
                ],
            )?;
            let menu = Menu::with_items(app, &[&app_submenu, &edit_submenu])?;
            app.set_menu(menu)?;
            Ok(())
        })
        .on_menu_event(|app, event| {
            if event.id().as_ref() == OPEN_ABOUT_ID {
                let _ = app.emit(OPEN_ABOUT_ID, ());
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_profiles,
            commands::create_profile,
            commands::regenerate_launchers,
            commands::update_profile,
            commands::reorder_profiles,
            commands::delete_profile,
            commands::toggle_surface,
            commands::open_profile_in_app,
            commands::open_in_finder,
            commands::profile_paths,
            commands::detect_existing_claude_install,
            commands::detect_existing_claude_sizes,
            commands::import_existing_install,
            commands::list_migration_backups,
            commands::delete_migration_backup,
            commands::check_dependencies,
            commands::detect_shell,
            commands::install_path_hook,
            commands::load_app_state,
            commands::update_app_state,
            commands::list_activity,
            commands::record_activity,
            commands::get_app_metadata,
            commands::open_external_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
