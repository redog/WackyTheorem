use tauri::{Window, Emitter};
use serde::{Deserialize, Serialize};

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
