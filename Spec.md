# Spec: WackyTheorem

> *An unconventional personal data assistant, conceived by an LLM, built for humans,
> executed with a complete lack of seriousness and a full complement of engineering rigour.*

## Goal

A desktop application that securely ingests data from the user's own accounts and devices,
stores it encrypted on-device, and eventually lets the user ask natural-language questions
about their own life.

Phase 1 success looks like: a running Tauri app that can authenticate with Google,
pull some data, encrypt it locally, and display it. The UX can be ugly. The data model
can be provisional. The important thing is that the pipeline exists end-to-end and nothing
lives in plaintext on disk.

## Stack (hard constraints)

- **Shell**: Tauri 2.x (Rust backend, Svelte frontend)
- **Language**: Rust (stable toolchain, current edition)
- **Database**: SQLite via `rusqlite` — one encrypted file per user, key derived from
  device secret. `sqlcipher` is the preferred encryption layer; if that proves painful,
  document why and open an issue before switching.
- **Auth**: Google OAuth 2.0 via PKCE flow — no client secrets stored in the binary.
  Tokens stored in the OS keychain (Tauri's `stronghold` or `keyring` plugin).
- **Frontend**: Svelte 5 — no external UI component library required for Phase 1.
  Plain HTML/CSS is fine. It just has to work.
- **CI**: GitHub Actions — `cargo build --release` and `cargo test` must pass on ubuntu-latest.
  No Windows or macOS CI required for Phase 1.

## Out of scope (Phase 1)

- The local LLM layer (Phase 2)
- Browser plugin / browser data source (Phase 2 prerequisite, not Phase 1)
- Mobile (Phase 3+)
- Apple Health (Phase 4)
- Polish, charts, or anything the Roadmap attributes to Phase 5
- Multi-user support
- Cloud sync of any kind

## What Google data, exactly?

Phase 1 is intentionally loose here — the goal is a working OAuth flow and at least
one ingested data type, not a complete Google integration. Suggested starting point:
**Google Calendar events** (well-documented API, structured, temporally interesting,
directly useful in Phase 2 queries). If a different Google data type turns out to be
easier to start with, that's fine — document the decision in `DECISIONS.md`.

## Definition of done (Phase 1)

Checkable criteria, in order:

1. `cargo build --release` succeeds on a clean Ubuntu machine with no manual setup
   beyond `rustup` and `npm install`.
2. Running the app presents a "Connect Google Account" button.
3. Clicking it opens a browser to the Google OAuth consent screen (PKCE flow, no
   client secret in binary).
4. After consent, the app stores the token in the OS keychain — not in a config file,
   not in plaintext anywhere.
5. The app pulls at least 30 days of data from one Google API endpoint (Calendar
   suggested) and writes it to the local SQLite database.
6. The database file on disk is encrypted — `file` command should show it as binary,
   not SQLite header.
7. The app has a view (however basic) that reads from the database and displays
   the ingested records to confirm the pipeline works.
8. CI passes: `cargo build --release` + `cargo test` green on ubuntu-latest.

## Intentional ambiguity (by design)

This project was conceived by an LLM. The brief was loose. That is a feature, not a bug.

The agent building this should make opinionated decisions where the spec is silent,
document those decisions in `DECISIONS.md`, and flag anything that feels like a
load-bearing choice in `Roadmap.md` under "Open questions for operator."

Eric will read the Roadmap. He will intervene if something is going wrong. The agent
should not stall waiting for perfect requirements — make a call, ship it, let Eric react.

## Constraints Eric has strong feelings about

- Nothing in plaintext on disk that shouldn't be. This is non-negotiable.
- No unnecessary dependencies. Every new crate needs a comment in `Cargo.toml`
  explaining why it's there.
- Auth and encryption changes require Eric's review before merge. Label the PR.
- The app should build on Linux. macOS support is nice. Windows is not a priority.
- `trash` over `rm` when agents are doing filesystem work. Recoverable beats gone.

## Repo

`https://github.com/redog/WackyTheorem`

Commits authored by `imp <imp@automationwise.com>`.
Eric's GitHub account (`redog`) is the repo owner and GitHub identity.
