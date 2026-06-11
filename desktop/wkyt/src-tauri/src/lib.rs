pub mod google_auth;
pub mod lifegraph;

use tauri::Manager;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Cold-start key/vault bootstrap (D12). Blocking: call from a blocking
/// thread. Returns an error string suitable for logging — never key bytes.
#[cfg(debug_assertions)]
fn open_vault(
    data_dir: &std::path::Path,
    db_path: &std::path::Path,
) -> Result<wkyt_vault::Vault, String> {
    use wkyt_vault::{unlock_vault, KeyService, KeyState, KeyringStore, Vault};

    let svc = KeyService::new(KeyringStore::new("wkyt"), data_dir);
    match svc.state(db_path.exists()).map_err(|e| e.to_string())? {
        KeyState::FirstRun => {
            let (dek, _recovery) = svc.provision().map_err(|e| e.to_string())?;
            // The D8 recovery ceremony UI lands with the viewer work; until
            // then the recovery key is deliberately dropped UNDISPLAYED
            // (never logged — that would put a secret in plaintext logs).
            // Acceptable only while this path is debug-only dev scaffolding.
            eprintln!(
                "[wkyt] vault provisioned. Recovery ceremony UI is pending — \
                 until it ships, keychain loss means data loss on this profile."
            );
            Vault::open(db_path, &dek).map_err(|e| e.to_string())
        }
        KeyState::Ready => {
            let (vault, _dek) = unlock_vault(&svc, db_path).map_err(|e| e.to_string())?;
            Ok(vault)
        }
        KeyState::KeychainLost => Err(
            "OS keychain entry is gone; recovery-key entry UI is pending (M4)".to_string()
        ),
        KeyState::Inconsistent(why) => Err(format!("key state inconsistent: {why}")),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Get the app data directory
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("failed to create app data dir");
            let db_path = app_data_dir.join("vault.db");

            println!("Vault path: {:?}", db_path);

            // Debug orchestrator (M3/M4): poll an import folder and run the
            // full pipeline — FileImporter -> bounded bus -> encrypted
            // vault, ack-after-commit, cursor resume across app restarts.
            #[cfg(debug_assertions)]
            tauri::async_runtime::spawn(async move {
                use std::sync::{Arc, Mutex};
                use wkyt_connector_file::FileImporter;

                let dir = app_data_dir.clone();
                let db = db_path.clone();
                let vault = match tauri::async_runtime::spawn_blocking(move || {
                    open_vault(&dir, &db)
                })
                .await
                {
                    Ok(Ok(v)) => Arc::new(Mutex::new(v)),
                    Ok(Err(e)) => {
                        eprintln!("[wkyt] vault unavailable, ingestion skipped: {e}");
                        return;
                    }
                    Err(e) => {
                        eprintln!("[wkyt] vault open task failed: {e}");
                        return;
                    }
                };

                let import_dir = app_data_dir.join("import");
                let connector = FileImporter::new("file-import", import_dir.clone());
                println!("[wkyt] watching {:?} — drop .json/.ics files there", import_dir);

                loop {
                    match wkyt_host::run_pipeline_once(&connector, Arc::clone(&vault)).await {
                        Ok(stats) if stats.batches_applied > 0 => {
                            let live = vault.lock().unwrap().item_count().unwrap_or(-1);
                            println!(
                                "[wkyt] ingested {} deltas in {} batches; vault holds {} live items",
                                stats.deltas_applied, stats.batches_applied, live
                            );
                        }
                        Ok(_) => {} // quiet pass, nothing changed
                        Err(e) => eprintln!("[wkyt] pipeline pass failed: {e}"),
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                }
            });

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            google_auth::start_oauth,
            google_auth::logout,
            google_auth::exchange_code_for_token,
            google_auth::get_user_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
