mod commands;
mod error;
mod launchers;
mod paths;
mod profiles;
mod slug;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            commands::list_profiles,
            commands::create_profile,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
