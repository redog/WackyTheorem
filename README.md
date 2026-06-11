# WackyTheorem : Promptware
[![Tauri CI](https://github.com/redog/WackyTheorem/actions/workflows/ci.yml/badge.svg)](https://github.com/redog/WackyTheorem/actions/workflows/ci.yml)

An unconventional app done with a complete lack of seriousness.

Built with Tauri and Svelte, designed to be a personal data assistant. Data is
ingested from local sources (and soon Google), encrypted on-device with
sqlcipher, and browsable in a minimal dashboard. Architectural decisions are
recorded in [`DECISIONS.md`](DECISIONS.md); the phase plan lives in
[`Roadmap.md`](Roadmap.md).

## Implementation Status

The project is in **Phase 1: Core Infrastructure & Unified Data Vault**.
The ingestion pipeline is complete end to end; Google OAuth + Calendar
ingestion is the remaining Phase 1 work.

**What works today:**

*   **Encrypted vault** (`wkyt-vault`): SQLite + sqlcipher (compiled from
    source, no system deps), KEK/DEK key hierarchy anchored to the OS
    keychain, recovery-key wrapper, crash-safe DEK rotation via
    `PRAGMA rekey`. Nothing lands on disk in plaintext — including SQLite
    temp storage.
*   **First-run recovery ceremony**: the app displays a one-time recovery
    key and forces re-entry verification before any data is ingested
    (D8). Keychain loss is recoverable in-app with that key.
*   **Connector contract** (`wkyt-core`): streaming, batched
    `sync(cursor)` with opaque connector-defined sync tokens, an error
    taxonomy (retryable / auth-required / resync-required / fatal),
    tombstones for deletions, and deterministic UUIDv5 item identity so
    re-ingestion is idempotent (D13). Protobuf wire format for batches.
*   **Pipeline** (`wkyt-broker` + `wkyt-host`): bounded in-process bus
    with backpressure; the orchestrator applies each batch and its cursor
    in one vault transaction and acks only after commit. Crash mid-batch
    resumes from the last committed cursor without duplicates.
*   **File importer** (`wkyt-connector-file`): watches an `import/`
    folder for `.json`/`.ics` files; modifications update in place,
    deletions tombstone, copied-in files with old mtimes are still caught.
*   **Viewer**: a Svelte dashboard listing vault items with auto-refresh
    (Spec DoD #7).
*   **CI**: build + full test suite on Linux, build on macOS/Windows.

**What is missing (rest of Phase 1):**

*   A real Google OAuth flow. The current backend commands are debug-mode
    mocks. The production flow will use OAuth 2.0 with PKCE (D5) — a
    `client_id` is required, but **no client secret**: PKCE replaces it,
    and the Spec forbids storing client secrets in the binary. Tokens
    will be stored in the OS keychain (D3).
*   Google Calendar ingestion (D4) as the second connector.
*   Headless-Linux fallback for machines without a Secret Service
    (passphrase + Argon2id per D2).

## Architecture

```
crates/
  wkyt-core            domain types, Connector trait, delta wire format
  wkyt-vault           encrypted vault: keys, schema, atomic batch-apply
  wkyt-broker          Bus trait + bounded in-process transport
  wkyt-connector-file  file-importer connector
  wkyt-host            orchestrator: connector -> bus -> vault, ack-after-commit
desktop/wkyt           Tauri app (Rust backend + Svelte frontend)
```

## How to Build and Run

Prerequisites: [Node.js](https://nodejs.org/en/),
[Rust](https://www.rust-lang.org/tools/install), and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your
platform. No other system libraries are required — sqlcipher and its
OpenSSL are compiled from source.

```bash
git clone https://github.com/redog/WackyTheorem.git
cd WackyTheorem/desktop/wkyt
npm install
npm run tauri dev
```

On first launch the app walks you through the recovery-key ceremony, then
shows the dashboard. Drop `.json` or `.ics` files into the `import/`
folder it displays (inside the app data directory) — they are ingested
into the encrypted vault within ~10 seconds.

## How to Test

```bash
cargo test --workspace        # from the repo root; 57 tests
cd desktop/wkyt && npm run check   # frontend type-check
```

Coverage includes the crypto lifecycle (provision / unlock / recovery /
rotation, tamper and blob-swap rejection), crash-recovery replay without
duplicates, and an end-to-end file → bus → encrypted-vault pipeline.

## Configuration

Google authentication (upcoming) uses OAuth 2.0 with PKCE (RFC 7636) —
see `DECISIONS.md` D5. You will need to supply your own Google OAuth
`client_id` (a "Desktop app" credential from the Google Cloud Console).
**Do not** create or embed a client secret: PKCE replaces it for native
apps, and the Spec forbids client secrets in the binary. Tokens are never
written to config files; they are stored in the OS keychain (D3).
