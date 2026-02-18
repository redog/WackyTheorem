pub mod google_auth;
pub mod lifegraph; // This line is crucial—it makes the compiler see the file above
pub mod storage;

use lifegraph::{Connector, MockConnector};
use storage::{DuckDbStorage, Storage};
use tauri::Manager;
use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Get the app data directory
            let app_data_dir = app.path().app_data_dir().expect("failed to get app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("failed to create app data dir");
            let db_path = app_data_dir.join("lifegraph.db");

            println!("Initializing database at: {:?}", db_path);

            // Initialize storage
            let storage = Arc::new(DuckDbStorage::new(db_path));
            if let Err(e) = storage.init() {
                eprintln!("Failed to init storage: {}", e);
            }

            // Placeholder: Initialize a mock connector
            tauri::async_runtime::spawn(async move {
                let connector = MockConnector { id: "test-conn-01".to_string() };
                match connector.init().await {
                    Ok(_) => {
                        println!("Connector init success");
                        let items = connector.full_sync().await.unwrap_or_else(|e| {
                            eprintln!("Sync failed: {}", e);
                            Vec::new() 
                        });
                        println!("Ingested {} items from mock connector.", items.len());

                        // Save items to DuckDB
                        if let Err(e) = storage.save_items(&items) {
                            eprintln!("Failed to save items to DB: {}", e);
                        } else {
                            println!("Saved {} items to DB.", items.len());
                        }
                    }
                    Err(e) => eprintln!("Connector init failed: {}", e),
                }
            });

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![]) 
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
