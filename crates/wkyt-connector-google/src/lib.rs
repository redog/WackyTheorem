//! Google Calendar connector (D3–D6).
//!
//! Implements the [`Connector`] trait for Google Calendar events. The auth
//! flow (OAuth 2.0 PKCE) is decoupled: if no valid tokens exist, `sync()`
//! returns [`SyncError::AuthRequired`] and the orchestrator/UI is
//! responsible for triggering the PKCE flow via [`auth::PkceFlow`] and
//! persisting tokens via [`auth::TokenStore`].
//!
//! # Architecture
//!
//! ```text
//!                         ┌──────────────────────────┐
//!   Tauri UI              │   GoogleCalendarConnector │
//!   ───────►  auth::      │                          │
//!             PkceFlow    │  sync(cursor)            │
//!             ──────►     │    ├─ token_store.access_token()
//!             TokenStore  │    │   └─ keyring or refresh
//!                         │    └─ calendar::fetch_calendar_events()
//!                         │        └─ reqwest → Calendar API v3
//!                         └──────────────────────────┘
//! ```

pub mod auth;
pub mod calendar;

use auth::TokenStore;
use futures_util::{stream, StreamExt as _};
use std::sync::Arc;
use wkyt_core::{Connector, DeltaStream, SyncError, SyncToken};

const CONNECTOR_ID: &str = "google-calendar";
const DEFAULT_BATCH_SIZE: usize = 100;

/// Google Calendar connector. Holds shared state (token store, HTTP client)
/// and exposes the standard [`Connector`] interface.
pub struct GoogleCalendarConnector {
    token_store: Arc<TokenStore>,
    http: reqwest::Client,
    batch_size: usize,
}

impl GoogleCalendarConnector {
    /// Create a new connector.
    ///
    /// `client_id` is the Google OAuth client ID (from Google Cloud Console).
    /// It is NOT a secret — PKCE replaces the client secret.
    pub fn new(client_id: impl Into<String>) -> Self {
        Self {
            token_store: Arc::new(TokenStore::new(client_id)),
            http: reqwest::Client::new(),
            batch_size: DEFAULT_BATCH_SIZE,
        }
    }

    /// Access the token store for auth flow operations.
    pub fn token_store(&self) -> &Arc<TokenStore> {
        &self.token_store
    }
}

#[async_trait::async_trait]
impl Connector for GoogleCalendarConnector {
    fn id(&self) -> &str {
        CONNECTOR_ID
    }

    /// Load cached tokens from the OS keychain. Cheap if already loaded.
    async fn init(&self) -> Result<(), SyncError> {
        // spawn_blocking because keyring ops are synchronous
        let store = self.token_store.clone();
        tokio::task::spawn_blocking(move || store.load_from_keyring())
            .await
            .map_err(|e| SyncError::Fatal {
                source: format!("join error: {e}").into(),
            })?
            .map_err(|e| SyncError::Fatal {
                source: e.into(),
            })?;
        Ok(())
    }

    /// Stream calendar event batches.
    ///
    /// If no valid access token is available, yields a single
    /// `AuthRequired` error — the orchestrator should pause this connector
    /// and surface the auth UI.
    fn sync(&self, cursor: Option<SyncToken>) -> DeltaStream<'_> {
        let http = self.http.clone();
        let token_store = self.token_store.clone();
        let connector_id = CONNECTOR_ID.to_string();
        let batch_size = self.batch_size;

        Box::pin(stream::once(async move {
            // Get a valid access token (may silently refresh).
            let access_token = token_store
                .access_token()
                .await
                .map_err(|e| SyncError::Fatal { source: e.into() })?
                .ok_or_else(|| SyncError::AuthRequired {
                    reason: "no Google OAuth tokens — user must authenticate".into(),
                })?;

            // Fetch all pages from Google and return batches.
            let batches = calendar::fetch_calendar_events(
                &http,
                &access_token,
                &connector_id,
                cursor,
                batch_size,
            )
            .await?;

            Ok(batches)
        })
        // flatten: the single future returns Vec<DeltaBatch>, we need to
        // stream each batch individually.
        .flat_map(|result| match result {
            Ok(batches) => {
                let items: Vec<Result<_, SyncError>> = batches.into_iter().map(Ok).collect();
                stream::iter(items).left_stream()
            }
            Err(e) => stream::iter(vec![Err(e)]).right_stream(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_id_matches_item_test_expectations() {
        let c = GoogleCalendarConnector::new("test-client-id");
        assert_eq!(c.id(), "google-calendar");
    }
}
