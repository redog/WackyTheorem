use serde::{Deserialize, Serialize};
use tauri::{Emitter, Window};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GoogleUser {
    pub email: String,
    pub name: String,
    pub picture: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct OAuthCodePayload {
    pub code: String,
    pub state: String,
}

// Basic auth state holder (placeholder for real OAuth flow)
pub struct AuthState {
    pub current_user: Option<GoogleUser>,
}

impl AuthState {
    pub fn new() -> Self {
        Self { current_user: None }
    }
}

#[tauri::command]
pub fn start_oauth(window: Window, state: String) -> Result<(), String> {
    #[cfg(debug_assertions)]
    {
        // This is where we would trigger the OIDC flow
        // For now, we just emit a mock oauth-code event
        let _ = window.emit(
            "oauth-code",
            OAuthCodePayload {
                code: "mock-code-123".to_string(),
                state,
            },
        );
        Ok(())
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = window;
        let _ = state;
        // In production, real OAuth flow must be implemented.
        Err("OAuth flow is not implemented for production builds yet.".to_string())
    }
}

#[tauri::command]
pub fn logout() {
    println!("Logging out");
}

#[tauri::command]
pub fn exchange_code_for_token(code: String) -> Result<String, String> {
    #[cfg(debug_assertions)]
    {
        Ok(format!("mock-token-for-{}", code))
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = code;
        // In production, real token exchange must be implemented.
        Err("Token exchange is not implemented for production builds yet.".to_string())
    }
}

#[tauri::command]
pub fn get_user_info(token: String) -> Result<GoogleUser, String> {
    #[cfg(debug_assertions)]
    {
        // In debug mode, return mock user data
        println!("Getting user info for token: {}", token);
        Ok(GoogleUser {
            email: "test@example.com".to_string(),
            name: "Test User".to_string(),
            picture: None,
        })
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = token;
        // In production, we must not return hardcoded mock data.
        // Real implementation of token verification and user info fetching is required.
        Err("Google user info fetching is not implemented for production builds yet.".to_string())
    }
}

pub fn initiate_auth(window: &Window) {
    #[cfg(debug_assertions)]
    {
        // This is where we would trigger the OIDC flow
        // For now, we just emit a mock success event
        let mock_user = GoogleUser {
            email: "demo@wkyt.app".to_string(),
            name: "Demo User".to_string(),
            picture: None,
        };

        let _ = window.emit("auth-success", mock_user);
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = window;
        // In production, real auth initiation must be implemented.
    }
}
