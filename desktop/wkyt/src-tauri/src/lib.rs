pub mod google_auth;
pub mod lifegraph; // This line is crucial—it makes the compiler see the file above

use lifegraph::{Connector, MockConnector};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|_app| {
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
