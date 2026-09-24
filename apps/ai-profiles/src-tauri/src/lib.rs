// SPDX-License-Identifier: MIT

mod accounts;
mod app_kind;
mod app_state;
pub mod cli;
mod commands;
mod deps;
mod error;
mod launch;
mod launchers;
mod migration;
mod path_setup;
mod paths;
mod profiles;
mod sessions;
mod shared_config;
mod slug;
#[cfg(test)]
mod test_support;
mod usage;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::Emitter;

/// Custom menu identifier for the App menu's "About ai-profiles" entry.
/// The default macOS About item opens a tiny system panel; we replace it
/// with an item that emits an event the frontend listens for, so the
/// click opens our own Atelier-styled `<AboutDialog>` instead.
const OPEN_ABOUT_ID: &str = "open-about";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Provides `relaunch()` to the frontend so the updater can restart
        // the app itself after `downloadAndInstall` finishes — Tauri 2's
        // updater plugin does NOT relaunch on its own.
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // Bring the desktop launchers an earlier version built up to this
            // one, so every profile changes over now rather than whenever it
            // next happens to be rebuilt. Off the main thread: a wrapper takes
            // seconds to build.
            std::thread::spawn(|| {
                let Ok(all) = profiles::load() else {
                    return;
                };
                let version = env!("CARGO_PKG_VERSION");
                for (id, result) in launchers::gui::refresh_outdated(&all, version) {
                    match result {
                        Ok(()) => eprintln!("ai-profiles: rebuilt the launcher of profile {id}"),
                        Err(err) => {
                            eprintln!("ai-profiles: left the launcher of profile {id}: {err}")
                        }
                    }
                }
                // Archives made before archives were compressed.
                for line in sessions::compress_old_archives() {
                    eprintln!("ai-profiles: {line}");
                }
            });

            // Build the macOS app menu manually so we can swap the default
            // About panel for a frontend-driven dialog. Everything else here
            // mirrors what Tauri would auto-generate (Services, Hide,
            // Hide Others, Show All, Quit) so the menu stays familiar.
            let about =
                MenuItem::with_id(app, OPEN_ABOUT_ID, "About ai-profiles", true, None::<&str>)?;
            let app_submenu = Submenu::with_items(
                app,
                "ai-profiles",
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
            commands::touch_profile_last_used,
            commands::open_in_finder,
            commands::open_default_gui,
            commands::profile_paths,
            commands::profile_account,
            commands::detect_existing_install,
            commands::detect_existing_sizes,
            commands::import_existing_install,
            commands::list_migration_backups,
            commands::delete_migration_backup,
            commands::check_dependencies,
            commands::detect_shell,
            commands::install_path_hook,
            commands::load_app_state,
            commands::update_app_state,
            commands::get_app_metadata,
            commands::get_profile_usage,
            commands::open_external_url,
            commands::open_cli_login,
            commands::list_sessions,
            commands::plan_session_transfer,
            commands::merge_transfer_memory,
            commands::transfer_session,
            commands::check_session_archive,
            commands::archive_session,
            commands::list_archived_sessions,
            commands::check_session_restore,
            commands::restore_session,
            commands::delete_archived_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
