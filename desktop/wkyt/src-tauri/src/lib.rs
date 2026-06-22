pub mod google_auth;
pub mod lifegraph;
pub mod vault_commands;

use std::sync::Arc;
use tauri::Manager;
use vault_commands::AppState;

#[cfg(target_family = "unix")]
fn disable_core_dumps() {
    unsafe {
        let rlim = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::setrlimit(libc::RLIMIT_CORE, &rlim) != 0 {
            eprintln!("[wkyt] warning: failed to disable core dumps");
        }
    }
}

#[cfg(not(target_family = "unix"))]
fn disable_core_dumps() {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    disable_core_dumps();

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

            // Google auth state: reads WKYT_GOOGLE_CLIENT_ID from env.
            // If unset, Google features are disabled gracefully.
            app.manage(Arc::new(google_auth::GoogleAuthState::new()));

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
            google_auth::google_auth_status,
            google_auth::start_oauth,
            google_auth::google_logout,
            google_auth::trigger_google_sync,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
