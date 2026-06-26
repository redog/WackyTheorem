//! OAuth 2.0 PKCE flow for Google (D5).
//!
//! Architecture: the auth module handles token acquisition and refresh but
//! does NOT own the Connector trait. The [`GoogleCalendarConnector`] calls
//! into this module to get a valid access token before making API requests.
//!
//! Token lifecycle:
//! 1. No tokens → `AuthRequired` error surfaces to the orchestrator/UI.
//! 2. UI calls [`PkceFlow::start`] → opens browser, catches redirect,
//!    exchanges code for tokens, stores in keyring.
//! 3. Connector calls [`TokenStore::access_token`] → returns cached token
//!    or silently refreshes using the refresh token.
//! 4. Refresh fails (revoked, expired) → `AuthRequired` again.

use keyring::Entry;
use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, CsrfToken, EndpointSet, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::RwLock;
use tracing::{debug, info, warn};

fn google_auth_url() -> String {
    std::env::var("WKYT_MOCK_GOOGLE_AUTH_URL")
        .unwrap_or_else(|_| "https://accounts.google.com/o/oauth2/v2/auth".to_string())
}

fn google_token_url() -> String {
    std::env::var("WKYT_MOCK_GOOGLE_TOKEN_URL")
        .unwrap_or_else(|_| "https://oauth2.googleapis.com/token".to_string())
}

/// Calendar read-only scope — the minimum we need for Phase 1.
const CALENDAR_SCOPE: &str = "https://www.googleapis.com/auth/calendar.readonly";

/// Keyring service name for WackyTheorem OAuth tokens.
const KEYRING_SERVICE: &str = "wkyt-google-oauth";
const KEYRING_USER: &str = "tokens";

/// Serialized token bundle stored in the OS keychain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix timestamp (seconds) when access_token expires.
    pub expires_at: Option<i64>,
}

impl StoredTokens {
    /// Conservative: treat tokens expiring within 60s as expired.
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(exp) => chrono::Utc::now().timestamp() >= (exp - 60),
            None => true, // no expiry info → assume expired, force refresh
        }
    }
}

use std::sync::Mutex;
static MOCK_KEYRING: Mutex<Option<String>> = Mutex::new(None);

fn is_keyring_available() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let entry = Entry::new(KEYRING_SERVICE, "availability-test");
        match entry {
            Ok(entry) => {
                match entry.set_password("test") {
                    Ok(()) => {
                        let _ = entry.delete_credential();
                        true
                    }
                    Err(keyring::Error::NoEntry) => true,
                    Err(_) => false,
                }
            }
            Err(_) => false,
        }
    })
}

/// Manages token persistence in the OS keychain and in-memory caching.
pub struct TokenStore {
    client_id: String,
    client_secret: Option<String>,
    cached: Arc<RwLock<Option<StoredTokens>>>,
}

impl TokenStore {
    pub fn new(client_id: impl Into<String>, client_secret: Option<impl Into<String>>) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret: client_secret.map(|s| s.into()),
            cached: Arc::new(RwLock::new(None)),
        }
    }

    /// Load tokens from keyring into cache. Call once at startup.
    pub fn load_from_keyring(&self) -> Result<bool, String> {
        if !is_keyring_available() {
            let guard = MOCK_KEYRING.lock().map_err(|e| e.to_string())?;
            if let Some(json) = guard.as_ref() {
                let tokens: StoredTokens = serde_json::from_str(json)
                    .map_err(|e| format!("corrupt token JSON in mock keyring: {e}"))?;
                let mut cache_guard = self.cached.write().unwrap();
                *cache_guard = Some(tokens);
                return Ok(true);
            }
            return Ok(false);
        }
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|e| format!("keyring entry error: {e}"))?;
        match entry.get_password() {
            Ok(json) => {
                let tokens: StoredTokens = serde_json::from_str(&json)
                    .map_err(|e| format!("corrupt token JSON in keyring: {e}"))?;
                let mut guard = self.cached.write().unwrap();
                *guard = Some(tokens);
                Ok(true)
            }
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(format!("keyring read error: {e}")),
        }
    }

    /// Persist tokens to keyring and update cache.
    pub async fn store(&self, tokens: StoredTokens) -> Result<(), String> {
        let json = serde_json::to_string(&tokens)
            .map_err(|e| format!("token serialization error: {e}"))?;

        if !is_keyring_available() {
            {
                let mut guard = MOCK_KEYRING.lock().map_err(|e| e.to_string())?;
                *guard = Some(json);
            }
            let mut cache_guard = self.cached.write().unwrap();
            *cache_guard = Some(tokens);
            return Ok(());
        }

        // keyring operations are blocking; run off the async runtime.
        let json_clone = json.clone();
        tokio::task::spawn_blocking(move || {
            let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
                .map_err(|e| format!("keyring entry error: {e}"))?;
            entry
                .set_password(&json_clone)
                .map_err(|e| format!("keyring write error: {e}"))
        })
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))??;

        let mut guard = self.cached.write().unwrap();
        *guard = Some(tokens);
        Ok(())
    }

    /// Get a valid access token. Refreshes silently if expired.
    /// Returns `None` if no tokens exist (user must authenticate).
    pub async fn access_token(&self) -> Result<Option<String>, String> {
        let tokens = {
            let guard = self.cached.read().unwrap();
            match guard.as_ref() {
                Some(t) => t.clone(),
                None => return Ok(None),
            }
        };

        if !tokens.is_expired() {
            return Ok(Some(tokens.access_token.clone()));
        }

        // Try refresh
        let refresh_token = match &tokens.refresh_token {
            Some(rt) => rt.clone(),
            None => {
                warn!("access token expired and no refresh token available");
                return Ok(None);
            }
        };

        debug!("access token expired, attempting refresh");
        match self.refresh(&refresh_token).await {
            Ok(new_tokens) => {
                let access = new_tokens.access_token.clone();
                self.store(new_tokens).await?;
                Ok(Some(access))
            }
            Err(e) => {
                warn!("token refresh failed: {e}");
                // Clear stale tokens so the next call surfaces AuthRequired
                let mut guard = self.cached.write().unwrap();
                *guard = None;
                Ok(None)
            }
        }
    }

    /// Clear stored tokens (logout).
    pub async fn clear(&self) -> Result<(), String> {
        if !is_keyring_available() {
            {
                let mut guard = MOCK_KEYRING.lock().map_err(|e| e.to_string())?;
                *guard = None;
            }
            let mut cache_guard = self.cached.write().unwrap();
            *cache_guard = None;
            return Ok(());
        }

        tokio::task::spawn_blocking(|| {
            if let Ok(entry) = Entry::new(KEYRING_SERVICE, KEYRING_USER) {
                let _ = entry.delete_credential();
            }
        })
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?;

        let mut guard = self.cached.write().unwrap();
        *guard = None;
        Ok(())
    }

    /// Refresh access token using the refresh token grant.
    async fn refresh(&self, refresh_token: &str) -> Result<StoredTokens, String> {
        let client = build_oauth_client(&self.client_id, self.client_secret.as_deref())?;
        let http_client = reqwest::Client::new();

        let token_result = client
            .exchange_refresh_token(&oauth2::RefreshToken::new(refresh_token.to_string()))
            .request_async(&http_client)
            .await
            .map_err(|e| format!("token refresh request failed: {e}"))?;

        let expires_at = token_result
            .expires_in()
            .map(|d| chrono::Utc::now().timestamp() + d.as_secs() as i64);

        Ok(StoredTokens {
            access_token: token_result.access_token().secret().to_string(),
            refresh_token: token_result
                .refresh_token()
                .map(|rt| rt.secret().to_string())
                .or_else(|| Some(refresh_token.to_string())), // Google doesn't always return a new one
            expires_at,
        })
    }
}

/// The PKCE authorization flow. Stateful: holds the verifier between
/// `authorize_url()` and `exchange()`.
pub struct PkceFlow {
    client_id: String,
    client_secret: Option<String>,
    pkce_verifier: Option<PkceCodeVerifier>,
    csrf_token: Option<CsrfToken>,
    redirect_port: u16,
}

impl PkceFlow {
    /// Prepare a new PKCE flow. Does not start the listener yet.
    pub fn new(client_id: &str, client_secret: Option<&str>, redirect_port: u16) -> Self {
        Self {
            client_id: client_id.to_string(),
            client_secret: client_secret.map(|s| s.to_string()),
            pkce_verifier: None,
            csrf_token: None,
            redirect_port,
        }
    }

    /// Generate the authorization URL. The caller should open this in the
    /// user's default browser.
    pub fn authorize_url(&mut self) -> Result<String, String> {
        let client = build_oauth_client(&self.client_id, self.client_secret.as_deref())?
            .set_redirect_uri(
                RedirectUrl::new(format!("http://localhost:{}/callback", self.redirect_port))
                    .map_err(|e| format!("invalid redirect URL: {e}"))?,
            );

        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();

        let (auth_url, csrf_token) = client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new(CALENDAR_SCOPE.to_string()))
            .add_extra_param("access_type", "offline") // get refresh token
            .add_extra_param("prompt", "consent") // force consent to ensure refresh token
            .set_pkce_challenge(challenge)
            .url();

        self.pkce_verifier = Some(verifier);
        self.csrf_token = Some(csrf_token);

        Ok(auth_url.to_string())
    }

    /// Start a localhost listener, wait for the redirect callback, and
    /// exchange the authorization code for tokens.
    ///
    /// Returns the stored tokens on success. The caller is responsible for
    /// persisting them via [`TokenStore::store`].
    pub async fn wait_for_callback_and_exchange(&mut self) -> Result<StoredTokens, String> {
        let verifier = self
            .pkce_verifier
            .take()
            .ok_or("authorize_url() must be called before exchange")?;
        let expected_state = self
            .csrf_token
            .take()
            .ok_or("authorize_url() must be called before exchange")?;

        // Bind the localhost listener
        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", self.redirect_port))
            .await
            .map_err(|e| format!("failed to bind localhost:{}: {e}", self.redirect_port))?;

        info!("listening for OAuth callback on localhost:{}", self.redirect_port);

        // Accept one connection
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|e| format!("failed to accept connection: {e}"))?;

        // Read the HTTP request
        let mut buf = vec![0u8; 4096];
        stream.readable().await.map_err(|e| format!("stream not readable: {e}"))?;
        let n = stream.try_read(&mut buf).map_err(|e| format!("read error: {e}"))?;
        let request = String::from_utf8_lossy(&buf[..n]);

        // Parse the GET request line for query parameters
        let request_line = request
            .lines()
            .next()
            .ok_or("empty HTTP request")?;

        let path = request_line
            .split_whitespace()
            .nth(1)
            .ok_or("malformed HTTP request line")?;

        let url = url::Url::parse(&format!("http://localhost{path}"))
            .map_err(|e| format!("failed to parse callback URL: {e}"))?;

        // Extract code and state from query params
        let code = url
            .query_pairs()
            .find(|(k, _)| k == "code")
            .map(|(_, v)| v.to_string())
            .ok_or("no authorization code in callback")?;
        let state = url
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.to_string())
            .ok_or("no state in callback")?;

        // Verify CSRF state
        if state != *expected_state.secret() {
            // Send error response before returning
            let response = "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\n\r\n<html><body><h1>Authentication Failed</h1><p>Invalid state parameter. Please try again.</p></body></html>";
            let _ = stream.writable().await;
            let _ = stream.try_write(response.as_bytes());
            return Err("CSRF state mismatch — possible attack or stale callback".to_string());
        }

        // Send success response to the browser
        let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body><h1>Authentication Successful</h1><p>You can close this tab and return to WackyTheorem.</p></body></html>";
        let _ = stream.writable().await;
        let _ = stream.try_write(response.as_bytes());

        info!("received authorization code, exchanging for tokens");

        // Exchange code for tokens
        let client = build_oauth_client(&self.client_id, self.client_secret.as_deref())?
            .set_redirect_uri(
                RedirectUrl::new(format!("http://localhost:{}/callback", self.redirect_port))
                    .map_err(|e| format!("invalid redirect URL: {e}"))?,
            );
        let http_client = reqwest::Client::new();

        let token_result = client
            .exchange_code(AuthorizationCode::new(code))
            .set_pkce_verifier(verifier)
            .request_async(&http_client)
            .await
            .map_err(|e| format!("token exchange failed: {e}"))?;

        let expires_at = token_result
            .expires_in()
            .map(|d| chrono::Utc::now().timestamp() + d.as_secs() as i64);

        Ok(StoredTokens {
            access_token: token_result.access_token().secret().to_string(),
            refresh_token: token_result
                .refresh_token()
                .map(|rt| rt.secret().to_string()),
            expires_at,
        })
    }
}

/// Fully-configured OAuth2 client with auth + token endpoints set.
type ConfiguredClient = BasicClient<EndpointSet, oauth2::EndpointNotSet, oauth2::EndpointNotSet, oauth2::EndpointNotSet, EndpointSet>;

/// Build the shared OAuth2 client (no redirect URI — callers add their own).
fn build_oauth_client(client_id: &str, client_secret: Option<&str>) -> Result<ConfiguredClient, String> {
    let mut client = BasicClient::new(ClientId::new(client_id.to_string()))
        .set_auth_uri(
            AuthUrl::new(google_auth_url())
                .map_err(|e| format!("invalid auth URL: {e}"))?,
        )
        .set_token_uri(
            TokenUrl::new(google_token_url())
                .map_err(|e| format!("invalid token URL: {e}"))?,
        );
    if let Some(secret) = client_secret {
        client = client.set_client_secret(oauth2::ClientSecret::new(secret.to_string()));
    }
    Ok(client)
}

/// Find a free high port for the localhost redirect listener.
pub async fn find_free_port() -> Result<u16, String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("failed to bind ephemeral port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("failed to get local address: {e}"))?
        .port();
    drop(listener);
    Ok(port)
}
