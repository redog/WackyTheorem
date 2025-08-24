# WackyTheorem

An unconventional app built with Tauri and Svelte, designed to be a personal data assistant. This project is in its early stages and aims to provide a secure, on-device way to interact with your personal data.

[![Tauri CI](https://github.com/redog/WackyTheorem/actions/workflows/ci.yml/badge.svg)](https://github.com/redog/WackyTheorem/actions/workflows/ci.yml)

## Implementation Status

The project is currently in **Phase 1: Core Infrastructure & Unified Data Vault**.

The main goal of this phase is to establish the foundational data layer. This involves securely ingesting, encrypting, and storing data from multiple sources directly on the user's device.

**What has been implemented:**

*   A desktop application shell using Tauri (Rust backend) and Svelte (frontend).
*   A partially implemented Google OAuth flow on the backend. This includes the ability to initiate the OAuth process and exchange an authorization code for an access token.

**What is missing:**

*   The Google OAuth flow requires a `client_id` and `client_secret`. You will need to provide your own credentials in `desktop/wkyt/src-tauri/src/google_auth.rs` to make it functional.
*   Secure storage of the access token.
*   Data ingestion from Google services.
*   Connections to other data sources.
*   An encrypted local database.
*   A user interface for managing data sources and viewing ingested data.

## How to Build and Run

To build and run this project, you will need to have Node.js and Rust installed, along with the Tauri prerequisites.

### Prerequisites

*   [Node.js](https://nodejs.org/en/)
*   [Rust](https://www.rust-lang.org/tools/install)
*   [Tauri Prerequisites](https://tauri.app/v1/guides/getting-started/prerequisites)

### Steps

1.  **Clone the repository:**
    ```bash
    git clone https://github.com/redog/WackyTheorem.git
    cd WackyTheorem/desktop/wkyt
    ```

2.  **Install frontend dependencies:**
    ```bash
    npm install
    ```

3.  **Run the application in development mode:**
    ```bash
    npm run tauri dev
    ```

## How to Test

Currently, there are no automated tests for this project. Testing is done manually by running the application and verifying its functionality.

## Configuration

To use the Google OAuth functionality, you must provide your own `client_id` and `client_secret` in the `exchange_code_for_token` function within `desktop/wkyt/src-tauri/src/google_auth.rs`.

```rust
// in desktop/wkyt/src-tauri/src/google_auth.rs

#[tauri::command]
pub async fn exchange_code_for_token(code: String) -> Result<String, String> {
  let client = reqwest::Client::new();
  let params = [
    ("code", code),
    // Replace these with your own credentials
    ("client_id", "YOUR_CLIENT_ID".into()),
    ("client_secret", "YOUR_CLIENT_SECRET".into()),
    ("redirect_uri", "http://localhost:8000".into()),
    ("grant_type", "authorization_code".into()),
  ];
  // ...
}
```
