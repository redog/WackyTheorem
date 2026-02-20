use serde::{Deserialize, Serialize};
use tauri::{Emitter, Window};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GoogleUser {
    pub email: String,
    pub name: String,
    pub picture: Option<String>,
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
pub fn start_oauth(window: Window, _state: String) {
    // This is where we would trigger the OIDC flow
    // For now, we just emit a mock oauth-code event
    let _ = window.emit("oauth-code", "mock-code-123");
}

#[tauri::command]
pub fn logout() {
    println!("Logging out");
}

#[tauri::command]
pub fn exchange_code_for_token(code: String) -> String {
    format!("mock-token-for-{}", code)
}

#[tauri::command]
pub fn get_user_info(token: String) -> GoogleUser {
    // In a real app, verify token and fetch user info
    // For now, return the mock user expected by the frontend
    println!("Getting user info for token: {}", token);
    GoogleUser {
        email: "test@example.com".to_string(),
        name: "Test User".to_string(),
        picture: None,
    }
}

pub fn initiate_auth(window: &Window) {
    // This is where we would trigger the OIDC flow
    // For now, we just emit a mock success event
    let mock_user = GoogleUser {
        email: "demo@wkyt.app".to_string(),
        name: "Demo User".to_string(),
        picture: None,
    };

    let _ = window.emit("auth-success", mock_user);
}
