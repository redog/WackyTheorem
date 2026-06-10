pub mod google_auth;
pub mod lifegraph; // This line is crucial—it makes the compiler see the file above
pub mod storage;

use lifegraph::Connector;
#[cfg(debug_assertions)]
use lifegraph::MockConnector;
use std::sync::Arc;
use storage::{DuckDbStorage, Storage};
use tauri::Manager;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
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
            let db_path = app_data_dir.join("lifegraph.db");

            println!("Initializing database at: {:?}", db_path);

            // Initialize storage
            let storage = Arc::new(DuckDbStorage::new(db_path));
            if let Err(e) = storage.init() {
                eprintln!("Failed to init storage: {}", e);
            }

            // Placeholder: Initialize a mock connector
            #[cfg(debug_assertions)]
            tauri::async_runtime::spawn(async move {
                use futures_util::StreamExt;
                use lifegraph::Delta;

                let connector = MockConnector {
                    id: "test-conn-01".to_string(),
                };
                if let Err(e) = connector.init().await {
                    eprintln!("Connector init failed: {}", e);
                    return;
                }
                println!("Connector init success");

                // Full sync (no cursor yet). Batches are applied as they
                // arrive; cursor persistence lands with wkyt-vault in M2.
                let mut stream = connector.sync(None);
                while let Some(batch) = stream.next().await {
                    let batch = match batch {
                        Ok(b) => b,
                        Err(e) => {
                            eprintln!("Sync failed: {}", e);
                            break;
                        }
                    };
                    let items: Vec<_> = batch
                        .deltas
                        .into_iter()
                        .filter_map(|d| match d {
                            Delta::Upsert(item) => Some(item),
                            // Tombstone handling lands with wkyt-vault (M2).
                            Delta::Tombstone { .. } => None,
                        })
                        .collect();
                    println!("Ingested batch of {} items from mock connector.", items.len());

                    let storage_clone = Arc::clone(&storage);
                    let items_len = items.len();
                    let save_result = tauri::async_runtime::spawn_blocking(move || {
                        storage_clone.save_items(&items)
                    })
                    .await;

                    match save_result {
                        Ok(Ok(_)) => println!("Saved {} items to DB.", items_len),
                        Ok(Err(e)) => eprintln!("Failed to save items to DB: {}", e),
                        Err(e) => eprintln!("Failed to join blocking task: {}", e),
                    }
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
