# WackyTheorem : Promptware
[![Tauri CI](https://github.com/redog/WackyTheorem/actions/workflows/ci.yml/badge.svg)](https://github.com/redog/WackyTheorem/actions/workflows/ci.yml)

An unconventional app done with a complete lack of seriousness. 

Built with Tauri and Svelte, designed to be a personal data assistant. This project is in its early stages and aims to provide a secure, on-device way to interact with your personal data.


## Implementation Status

The project is currently in **Phase 1: Core Infrastructure & Unified Data Vault**.

The main goal of this phase is to establish the foundational data layer. This involves securely ingesting, encrypting, and storing data from multiple sources directly on the user's device.

**What has been implemented:**

*   A desktop application shell using Tauri (Rust backend) and Svelte (frontend).
*   A partially implemented Google OAuth flow on the backend. This includes the ability to initiate the OAuth process and exchange an authorization code for an access token.

**What is missing:**

*   A real Google OAuth flow. The current backend is a debug-mode mock. The
    production flow will use OAuth 2.0 with PKCE (see `DECISIONS.md` D5) —
    a `client_id` is required, but **no client secret**: PKCE replaces it,
    and the Spec forbids storing client secrets in the binary.
*   Secure storage of the access token (OS keychain via `keyring`, per D3).
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

Backend unit tests live in `desktop/wkyt/src-tauri/src` and run with
`cargo test` from `desktop/wkyt/src-tauri`. End-to-end behavior is still
verified manually by running the application.

## Configuration

Google authentication uses OAuth 2.0 with PKCE (RFC 7636) — see
`DECISIONS.md` D5. You will need to supply your own Google OAuth
`client_id` (a "Desktop app" credential from the Google Cloud Console).
**Do not** create or embed a client secret: PKCE replaces it for native
apps, and the Spec forbids client secrets in the binary. Tokens are never
written to config files; they are stored in the OS keychain (D3).
