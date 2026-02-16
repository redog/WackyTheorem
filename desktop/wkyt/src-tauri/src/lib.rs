pub mod google_auth;
pub mod lifegraph; // This line is crucial—it makes the compiler see the file above
pub mod storage;

use lifegraph::{Connector, MockConnector};
use storage::Storage;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Initialize Storage
            // Using a simple path for now. In a real app, use app.path().app_data_dir()
            // But getting app_data_dir requires resolving paths which might need configuration.
            // For simplicity and to ensure it runs without extra plugins configuration:
            let db_path = "wkyt.db";
            let storage = Arc::new(Storage::new(db_path).expect("failed to init storage"));

            app.manage(storage.clone());

            let storage_clone = storage.clone();

            // Placeholder: Initialize a mock connector
            tauri::async_runtime::spawn(async move {
                let connector = MockConnector { id: "test-conn-01".to_string() };
                match connector.init().await {
                    Ok(_) => {
                        println!("Connector init success");
                        // Fixed: unwrap_or_else handles the Result error correctly
                        let items = connector.full_sync().await.unwrap_or_else(|e| {
                            eprintln!("Sync failed: {}", e);
                            Vec::new() 
                        });
                        println!("Ingested {} items from mock connector.", items.len());

                        // Save items to storage
                        for item in items {
                            if let Err(e) = storage_clone.add_item(&item) {
                                eprintln!("Failed to save item {}: {}", item.id, e);
                            } else {
                                println!("Saved item {} to storage.", item.id);
                            }
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
