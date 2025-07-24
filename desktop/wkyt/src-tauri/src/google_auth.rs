use tauri::{Window, AppHandle, Manager, Emitter};
use tauri_plugin_oauth::{start_with_config, OauthConfig};

#[tauri::command]
pub fn start_oauth(window: Window, state: String) -> Result<u16, String> {
  let cfg = OauthConfig {
    ports: Some(vec![8000, 8001, 8002]), // avoid port conflicts
    response: Some("You may now close this page.".into()),
    ..Default::default()
  };
  start_with_config(cfg, move |url| {
    if let Some(code) = verify_callback(&url, &state) {
      let _ = window.emit("oauth-code", code);
    }
  })
  .map_err(|e| e.to_string())
}

fn verify_callback(url: &str, state: &str) -> Option<String> {
    let url = url::Url::parse(url).ok()?;
    let query_params = url.query_pairs().into_owned().collect::<Vec<(String, String)>>();

    let state_param = query_params.iter().find(|(k, _)| k == "state")?;
    if state_param.1 != state {
        return None;
    }

    let code_param = query_params.iter().find(|(k, _)| k == "code")?;
    Some(code_param.1.clone())
}

#[tauri::command]
pub async fn exchange_code_for_token(code: String) -> Result<String, String> {
  let client = reqwest::Client::new();
  let params = [
    ("code", code),
    ("client_id", "YOUR_CLIENT_ID".into()),
    ("client_secret", "YOUR_CLIENT_SECRET".into()),
    ("redirect_uri", "http://localhost:8000".into()),
    ("grant_type", "authorization_code".into()),
  ];
  let res = client.post("https://oauth2.googleapis.com/token")
    .form(&params)
    .send().await.map_err(|e| e.to_string())?;
  let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
  Ok(json["access_token"].as_str().unwrap_or_default().to_string())
}

#[tauri::command]
pub fn logout() {
    println!("logout called");
}
