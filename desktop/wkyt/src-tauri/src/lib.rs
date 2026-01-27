pub mod google_auth;
pub mod lifegraph; // Expose the new core module

use tauri::Manager;
use lifegraph::{Connector, MockConnector};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Placeholder: Initialize a mock connector to prove the trait works
            tauri::async_runtime::spawn(async move {
                let connector = MockConnector { id: "test-conn-01".to_string() };
                match connector.init().await {
                    Ok(_) => {
                        println!("Connector init success");
                        let items = connector.full_sync().await.unwrap_or_default();
                        println!("Ingested {} items from mock connector.", items.len());
                    }
                    Err(e) => eprintln!("Connector init failed: {}", e),
                }
            });

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![]) // No commands yet
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
