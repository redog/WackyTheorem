pub mod google_auth;
pub mod lifegraph;
pub mod vault_commands;

use std::sync::Arc;
use tauri::Manager;
use vault_commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("failed to create app data dir");

            // The vault lifecycle (unlock / first-run ceremony / recovery)
            // is driven by the frontend through vault_commands; nothing is
            // unlocked and no ingestion runs until the UI asks.
            app.manage(Arc::new(AppState::new(app_data_dir)));
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            vault_commands::vault_status,
            vault_commands::begin_first_run,
            vault_commands::verify_recovery_key,
            vault_commands::recover_with_key,
            vault_commands::get_items,
            vault_commands::get_stats,
            google_auth::start_oauth,
            google_auth::logout,
            google_auth::exchange_code_for_token,
            google_auth::get_user_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
