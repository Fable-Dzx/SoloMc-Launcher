//! SoloMC Tauri 入口

mod commands;
mod launcher;

pub use launcher::error;

use commands::GameState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(GameState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::fetch_remote_versions,
            commands::list_installed,
            commands::install_version,
            commands::loader_versions,
            commands::install_loader,
            commands::launch_instance,
            commands::stop_game,
            commands::offline_login,
            commands::microsoft_start_device_code,
            commands::microsoft_poll,
            commands::microsoft_logout,
            commands::modrinth_search,
            commands::modrinth_versions,
            commands::modrinth_install,
        ])
        .setup(|app| {
            // 确保目录存在
            let settings = launcher::settings::Settings::load();
            let _ = std::fs::create_dir_all(settings.game_dir.join("versions"));
            let _ = std::fs::create_dir_all(settings.game_dir.join("libraries"));
            let _ = std::fs::create_dir_all(settings.game_dir.join("assets").join("indexes"));
            let _ = std::fs::create_dir_all(settings.game_dir.join("assets").join("objects"));
            let _ = std::fs::create_dir_all(settings.game_dir.join("natives"));
            let _ = app.manage(settings);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
