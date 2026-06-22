//! Google OAuth 2.0 PKCE commands (D3/D5).
//!
//! The real flow:
//! 1. Frontend calls `start_oauth` → spawns PKCE flow, opens browser.
//! 2. User consents → localhost callback → tokens exchanged and stored.
//! 3. Frontend calls `google_auth_status` to check if tokens exist.
//! 4. On vault READY, the Google Calendar connector joins the pipeline.
//!
//! Client ID is read from the `WKYT_GOOGLE_CLIENT_ID` env var at runtime.
//! It is NOT a secret (Google's installed-app docs explicitly say this),
//! but we don't compile it into the binary so users can bring their own
//! Google Cloud project.

use serde::Serialize;
use std::sync::Arc;
use wkyt_connector_google::auth;
use wkyt_connector_google::GoogleCalendarConnector;

/// OAuth status returned to the frontend.
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GoogleAuthStatus {
    /// No client ID configured — Google features disabled.
    NotConfigured,
    /// Client ID present but no tokens — user needs to authenticate.
    NeedsAuth,
    /// Tokens present and (possibly) valid.
    Authenticated { email: Option<String> },
}

/// Shared state for Google auth, managed alongside AppState.
pub struct GoogleAuthState {
    client_id: Option<String>,
    token_store: Option<Arc<auth::TokenStore>>,
}

impl GoogleAuthState {
    pub fn new() -> Self {
        let client_id = std::env::var("WKYT_GOOGLE_CLIENT_ID").ok();
        let token_store = client_id
            .as_ref()
            .map(|id| Arc::new(auth::TokenStore::new(id.clone())));

        Self {
            client_id,
            token_store,
        }
    }

    pub fn client_id(&self) -> Option<&str> {
        self.client_id.as_deref()
    }

    pub fn token_store(&self) -> Option<&Arc<auth::TokenStore>> {
        self.token_store.as_ref()
    }
}

/// Check the current Google auth status.
#[tauri::command]
pub async fn google_auth_status(
    state: tauri::State<'_, Arc<GoogleAuthState>>,
) -> Result<GoogleAuthStatus, String> {
    let s = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || {
        let Some(store) = s.token_store() else {
            return Ok(GoogleAuthStatus::NotConfigured);
        };

        match store.load_from_keyring() {
            Ok(true) => Ok(GoogleAuthStatus::Authenticated { email: None }),
            Ok(false) => Ok(GoogleAuthStatus::NeedsAuth),
            Err(e) => {
                eprintln!("[wkyt] keyring read warning: {e}");
                Ok(GoogleAuthStatus::NeedsAuth)
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Start the OAuth PKCE flow: find a free port, generate the auth URL,
/// open the browser, wait for the callback, exchange the code, store tokens.
///
/// This is a long-running command — the frontend should show a spinner
/// while waiting.
#[tauri::command]
pub async fn start_oauth(
    state: tauri::State<'_, Arc<GoogleAuthState>>,
    app_state: tauri::State<'_, Arc<crate::vault_commands::AppState>>,
) -> Result<GoogleAuthStatus, String> {
    let client_id = state
        .client_id()
        .ok_or("WKYT_GOOGLE_CLIENT_ID is not set")?
        .to_string();

    let store = state
        .token_store()
        .ok_or("Google auth not configured")?
        .clone();

    // Find a free port for the redirect listener
    let port = auth::find_free_port()
        .await
        .map_err(|e| format!("port allocation failed: {e}"))?;

    // Set up the PKCE flow
    let mut flow = auth::PkceFlow::new(&client_id, port);
    let auth_url = flow.authorize_url()
        .map_err(|e| format!("failed to generate auth URL: {e}"))?;

    // Open the browser
    open::that(&auth_url).map_err(|e| format!("failed to open browser: {e}"))?;

    // Wait for callback and exchange (this blocks until the user completes consent)
    let tokens = flow
        .wait_for_callback_and_exchange()
        .await
        .map_err(|e| format!("OAuth flow failed: {e}"))?;

    // Store tokens in keyring
    store
        .store(tokens)
        .await
        .map_err(|e| format!("failed to store tokens: {e}"))?;

    // Spawn a one-off sync run so the user gets their data immediately!
    if let Some(vault) = app_state.cached_vault() {
        let google = GoogleCalendarConnector::new(client_id.clone());
        tauri::async_runtime::spawn(async move {
            let _ = wkyt_host::run_pipeline_once(&google, vault).await;
        });
    }

    Ok(GoogleAuthStatus::Authenticated { email: None })
}

/// Clear stored Google tokens (logout).
#[tauri::command]
pub async fn google_logout(
    state: tauri::State<'_, Arc<GoogleAuthState>>,
) -> Result<(), String> {
    let store = state
        .token_store()
        .ok_or("Google auth not configured")?
        .clone();
    store.clear().await
}

/// Trigger an on-demand sync of Google Calendar events.
#[tauri::command]
pub async fn trigger_google_sync(
    state: tauri::State<'_, Arc<GoogleAuthState>>,
    app_state: tauri::State<'_, Arc<crate::vault_commands::AppState>>,
) -> Result<(), String> {
    let client_id = state
        .client_id()
        .ok_or("WKYT_GOOGLE_CLIENT_ID is not set")?
        .to_string();

    let vault = app_state
        .cached_vault()
        .ok_or("Vault is not unlocked")?;

    let google = GoogleCalendarConnector::new(client_id);
    
    wkyt_host::run_pipeline_once(&google, vault)
        .await
        .map_err(|e| format!("Google Calendar sync failed: {e}"))?;

    Ok(())
}
