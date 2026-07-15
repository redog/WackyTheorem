use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener as TokioTcpListener;
use url::Url;

use wkyt_connector_google::{
    auth::{PkceFlow, TokenStore, StoredTokens, find_free_port},
    GoogleCalendarConnector,
};
use wkyt_core::{Connector, ItemKind};
use wkyt_vault::{KeyService, MemoryKekStore, Vault};
use wkyt_host::run_pipeline_once;

// Helper to get a free port
fn get_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

#[tokio::test]
async fn test_oauth_pkce_flow() {
    let mock_token_port = get_free_port();
    let mock_token_url = format!("http://127.0.0.1:{}/token", mock_token_port);
    std::env::set_var("WKYT_MOCK_GOOGLE_TOKEN_URL", &mock_token_url);
    std::env::set_var("WKYT_MOCK_GOOGLE_AUTH_URL", "http://127.0.0.1:9999/auth");

    // Spawn mock Google Token Server
    let token_response = r#"{
        "access_token": "mock-access-token",
        "refresh_token": "mock-refresh-token",
        "expires_in": 3600,
        "token_type": "Bearer"
    }"#;
    
    let mock_server_handle = tokio::spawn(async move {
        let listener = TokioTcpListener::bind(format!("127.0.0.1:{}", mock_token_port)).await.unwrap();
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                token_response.len(),
                token_response
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
        }
    });

    let redirect_port = find_free_port().await.unwrap();
    let mut flow = PkceFlow::new("test-client-id", Some("test-client-secret"), redirect_port);
    
    let auth_url_str = flow.authorize_url().unwrap();
    let auth_url = Url::parse(&auth_url_str).unwrap();
    let state = auth_url.query_pairs().find(|(k, _)| k == "state").map(|(_, v)| v.into_owned()).unwrap();

    // Spawn task to simulate user browser redirecting to the local redirect port
    let callback_url = format!("http://127.0.0.1:{}/callback?code=test-code&state={}", redirect_port, state);
    let browser_sim = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let resp = reqwest::get(&callback_url).await.unwrap();
        assert!(resp.status().is_success());
    });

    // Run the exchange flow
    let tokens = flow.wait_for_callback_and_exchange().await.unwrap();
    assert_eq!(tokens.access_token, "mock-access-token");
    assert_eq!(tokens.refresh_token.as_deref(), Some("mock-refresh-token"));

    // Verify token storage in the token store
    let store = TokenStore::new("test-client-id", Some("test-client-secret"));
    store.store(tokens).await.unwrap();
    
    let loaded = store.load_from_keyring().unwrap();
    assert!(loaded);

    let token_opt = store.access_token().await.unwrap();
    assert_eq!(token_opt.as_deref(), Some("mock-access-token"));

    // Cleanup
    mock_server_handle.await.unwrap();
    browser_sim.await.unwrap();
}

#[tokio::test]
async fn test_google_calendar_ingestion_loop() {
    let mock_api_port = get_free_port();
    let mock_api_base = format!("http://127.0.0.1:{}", mock_api_port);
    std::env::set_var("WKYT_MOCK_CALENDAR_API_BASE", &mock_api_base);

    // Mock Calendar API response returning two events
    let calendar_response = r#"{
        "items": [
            {
                "id": "evt-id-1",
                "status": "confirmed",
                "summary": "Project Review Meeting",
                "description": "Discussing phase 1 deliverables",
                "location": "Conference Room A",
                "start": {
                    "dateTime": "2026-06-26T14:00:00Z"
                },
                "end": {
                    "dateTime": "2026-06-26T15:00:00Z"
                }
            },
            {
                "id": "evt-id-2",
                "status": "cancelled"
            }
        ],
        "nextSyncToken": "mock-next-sync-token"
    }"#;

    let mock_server_handle = tokio::spawn(async move {
        let listener = TokioTcpListener::bind(format!("127.0.0.1:{}", mock_api_port)).await.unwrap();
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                calendar_response.len(),
                calendar_response
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
        }
    });

    // Setup temporary vault
    let vault_dir = tempfile::tempdir().unwrap();
    let key_service = KeyService::new(MemoryKekStore::default(), vault_dir.path());
    let (dek, _recovery) = key_service.provision().unwrap();
    let vault = Arc::new(std::sync::Mutex::new(
        Vault::open(&vault_dir.path().join("vault.db"), &dek).unwrap()
    ));

    // Setup connector with pre-existing tokens
    let connector = GoogleCalendarConnector::new("test-client-id", Some("test-client-secret"));
    let store = connector.token_store();
    store.store(StoredTokens {
        access_token: "active-access-token".to_string(),
        refresh_token: Some("active-refresh-token".to_string()),
        expires_at: Some(chrono::Utc::now().timestamp() + 3600),
    }).await.unwrap();

    // Run pipeline once
    let stats = run_pipeline_once(&connector, vault.clone()).await.unwrap();
    
    // Assert statistics:
    // Only 1 event is confirmed (Upsert), which also emits a Claim and a Relationship.
    // The other is cancelled (Tombstone), yielding 1 delta.
    // Total deltas: 3 for the event + 1 tombstone = 4.
    assert_eq!(stats.batches_applied, 1);
    assert_eq!(stats.deltas_applied, 4);

    let v = vault.lock().unwrap();
    assert_eq!(v.item_count().unwrap(), 3); // 1 Event + 1 Claim + 1 Relationship

    // Verify properties of the ingested event
    let items = v.items("google-calendar").unwrap();
    let event = items.iter().find(|i| i.source_id == "evt-id-1").unwrap();
    assert_eq!(event.properties["summary"], "Project Review Meeting");
    assert_eq!(event.properties["location"], "Conference Room A");
    assert_eq!(event.kind, ItemKind::Event);

    // Check that the cursor has been persisted in the vault
    let cursor = v.cursor("google-calendar").unwrap().unwrap();
    assert!(cursor.0.contains("mock-next-sync-token"));

    mock_server_handle.await.unwrap();
}
