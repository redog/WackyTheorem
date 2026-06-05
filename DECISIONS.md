# DECISIONS.md

Append-only log of architectural and design decisions for WackyTheorem.
Each entry records what was decided, why, and what was rejected.

---

## D1: Database engine — SQLite/sqlcipher, not DuckDB

**Date:** 2025-06-05
**Status:** Decided (Spec constraint)
**Context:** The initial scaffold used DuckDB (`duckdb` crate, bundled).
The Spec hard-constrains storage to "SQLite via `rusqlite` — one encrypted
file per user, key derived from device secret. `sqlcipher` is the preferred
encryption layer."

**Decision:** Replace DuckDB with `rusqlite` using the `bundled-sqlcipher`
feature. This gives us SQLite + transparent encryption in a single crate
with no external C library dependency.

**What changes:**
- `duckdb` crate removed from Cargo.toml.
- `rusqlite` added with `features = ["bundled-sqlcipher"]`.
- `storage.rs` rewritten against rusqlite's API.
- The `lifegraph.rs` data model (Item, ItemKind, Connector trait) stays —
  it's storage-agnostic.
- Tests rewritten. The upsert syntax changes (rusqlite/SQLite uses
  `INSERT OR REPLACE` or `ON CONFLICT`; same semantics, different dialect
  from DuckDB 1.5+).

**Why not keep DuckDB:**
- Spec says SQLite. That's the end of the conversation.
- sqlcipher gives us file-level encryption for free. DuckDB has no
  equivalent. We'd have to bolt on application-layer encryption, which
  is more code, more bugs, more audit surface.
- SQLite is the more natural fit for a single-user desktop app with
  modest data volumes.

**Rejected alternatives:**
- DuckDB + application-layer encryption (complex, Spec says no).
- `libsqlite3-sys` with system sqlcipher (works, but `bundled-sqlcipher`
  is simpler for CI and cross-platform builds).

---

## D2: Encryption key strategy — random key in OS keychain

**Date:** 2025-06-05
**Status:** Revised 2025-06-05 (panel feedback incorporated)
**Context:** sqlcipher needs a key to encrypt the database. The Spec says
"key derived from device secret." We need to decide what the device secret
is and where it lives.

**Decision:** On first run, generate a 256-bit random key via the OS
CSPRNG. Store it in the OS keychain using the `keyring` crate. Pass it
to sqlcipher via `PRAGMA key` on every database open.

**Rationale:**
- A random key is the strongest option — no derivation weakness, no
  guessable inputs.
- The OS keychain is the right place for secrets on desktop. On Linux
  this backs to the Secret Service API (GNOME Keyring / KWallet). On
  macOS it's Keychain. Both are encrypted at rest and unlocked by the
  user's login session.
- The `keyring` crate is mature, well-tested, and maps directly to the
  OS secret service on each platform. Simpler and more transparent than
  Tauri's stronghold layer.
- No user passphrase required for Phase 1. The DB is protected by the
  OS login.

**Panel input (claude-bot, grok-bot, groq-bot):**
- Unanimous recommendation for `keyring` over stronghold.
- grok-bot flagged Linux keychain reliability as a real risk — GNOME
  Keyring / KWallet may not be present on headless or minimal installs.
- claude-bot recommended CI testing on multiple Linux environments
  (Ubuntu with GNOME, headless) and graceful degradation.
- See D8 for the recovery key mitigation.

**Graceful degradation (Linux):** If keyring is unavailable at runtime,
prompt the user for a passphrase. Derive key via Argon2id from
passphrase + random salt. Store salt in plaintext alongside DB. This
fallback means the app works on headless Linux without a secret service.

**Rejected alternatives:**
- Derive from machine ID (predictable, not a secret).
- User passphrase on every launch (UX friction for the common case).
- Hardcoded key (obviously not).
- `tauri-plugin-stronghold` (heavier, adds Tauri's own encrypted vault
  layer on top of the OS keychain — more complexity for the same result.
  Original D2 chose this; revised after panel consensus favored keyring).

---

## D3: OAuth token storage — keyring crate

**Date:** 2025-06-05
**Status:** Revised 2025-06-05 (panel feedback incorporated)
**Context:** After Google OAuth, we have an access token and refresh token
that must not be stored in plaintext. The Spec says "Tokens stored in the
OS keychain (Tauri's `stronghold` or `keyring` plugin)."

**Decision:** Use the `keyring` crate for token storage. Same backend as
the DB encryption key (D2).

**Rationale:**
- One secret store, not two. `keyring` already holds the DB key; adding
  tokens to the same keychain service is natural.
- Tokens are stored as serialized JSON in a single keyring entry
  (access_token, refresh_token, expiry, scope).
- Same graceful degradation as D2 — if keyring is unavailable, tokens
  can be stored in an encrypted file with the passphrase-derived key.

**Panel consensus:** All bots recommended `keyring` for both DB key and
tokens. Consistent with D2.

**Rejected alternatives:**
- `tauri-plugin-stronghold` (original D3 choice; heavier, adds its own
  vault layer; revised after panel consensus).
- Plaintext JSON in app data dir (Spec forbids this).
- Environment variables (fragile, not persistent).

---

## D4: Google data source — Calendar events

**Date:** 2025-06-05
**Status:** Decided (Spec suggestion, adopted)
**Context:** Spec says "at least one ingested data type" and suggests
Google Calendar. The Roadmap mentions "two data sources" but the Spec
explicitly puts browser plugin out of scope for Phase 1.

**Decision:** Phase 1 ingests Google Calendar events only. Browser
plugin is Phase 2 per Spec.

**Rationale:**
- Calendar API is well-documented, returns structured JSON, and the data
  is temporally indexed — exactly what Phase 2's LLM queries will want.
- 30 days of calendar events is a small, bounded dataset. Good for
  proving the pipeline without worrying about pagination complexity or
  rate limits.
- The `Item` model in lifegraph.rs already has `ItemKind::Event`, which
  maps naturally to calendar events.

**Roadmap/Spec mismatch noted:** The Roadmap says Phase 1 needs Google +
browser. The Spec says browser is out of scope. Following the Spec.
Roadmap should be updated to match.

---

## D5: OAuth flow — PKCE with localhost redirect

**Date:** 2025-06-05
**Status:** Decided
**Context:** Spec requires "Google OAuth 2.0 via PKCE flow — no client
secrets stored in the binary." Need to decide the redirect mechanism.

**Decision:** Use the `oauth2` crate with PKCE. Spin up a temporary
localhost HTTP server to catch the redirect. The redirect URI is
`http://localhost:<port>/callback` with a random high port.

**Flow:**
1. Generate PKCE code verifier + challenge.
2. Open the user's default browser to Google's authorization endpoint
   with `response_type=code`, the challenge, and the localhost redirect.
3. Temporary localhost server catches the callback with the auth code.
4. Exchange code + verifier for tokens server-side (from Rust, not the
   browser).
5. Store tokens in stronghold (D3).
6. Shut down the temporary server.

**Rationale:**
- This is the standard PKCE flow for desktop/native apps per Google's
  docs and RFC 7636.
- No client secret needed in the binary — PKCE replaces it.
- The `oauth2` crate handles the PKCE math, token exchange, and refresh.
  No hand-rolling crypto.
- Localhost redirect is simpler than custom URI schemes for Linux, and
  Google explicitly supports it for installed apps.

**Crate:** `oauth2` (well-maintained, 10M+ downloads, handles PKCE natively).

**Rejected alternatives:**
- Custom URI scheme (`wkyt://callback`) — requires OS-level registration,
  more complex on Linux, not needed for Phase 1.
- Manual HTTP requests without oauth2 crate (more code, more bugs, same
  result).
- Embedded webview for auth (Google discourages this for OAuth).

---

## D6: HTTP client — reqwest

**Date:** 2025-06-05
**Status:** Decided
**Context:** We need an HTTP client for the Google Calendar API calls
and for the OAuth token exchange.

**Decision:** Use `reqwest` with `features = ["json", "rustls-tls"]`.

**Rationale:**
- `oauth2` crate needs an async HTTP client. reqwest is the default
  pairing and the most common in the Rust ecosystem.
- rustls-tls avoids linking against system OpenSSL, which simplifies
  cross-platform builds.
- We'll also use reqwest directly for Calendar API calls.

**Rejected alternatives:**
- `ureq` (sync-only, but our Tauri app is already async).
- `hyper` directly (too low-level for API calls).
- System OpenSSL (`native-tls` feature — works but rustls is simpler
  for CI).

---

## D7: Phase 1 milestone order

**Date:** 2025-06-05
**Status:** Decided
**Context:** Five milestones identified. Need to sequence them.

**Decision:**
1. **M1: DB migration** (DuckDB → rusqlite/sqlcipher) — everything depends on this.
2. **M2: OAuth PKCE** — can't get data without auth.
3. **M3: Calendar ingestion** — the pipeline.
4. **M4: Data viewer** — prove it works.
5. **M5: CI green** — seal it.

M1 and M2 are independent in code but M2 is useless without somewhere
to store the tokens, and stronghold setup is part of M1's "secret store"
work. So M1 first, M2 second, then the rest is linear.

---

## D8: Recovery key ceremony on first run

**Date:** 2025-06-05
**Status:** Decided
**Context:** If the OS keychain is lost (OS reinstall, migration to new
machine, keyring corruption), the sqlcipher DB key is gone and all
encrypted data is unrecoverable. grok-bot flagged this as a hidden
product risk: "users will treat the app as their calendar source of
truth. When the keychain dies, they lose months of data with no warning.
That's not just a support ticket — it's a 'this app ate my data'
reputation hit." claude-bot agreed and changed their vote.

**Decision:** On first database creation, show a modal with a recovery
key. The user must acknowledge it (copy or download) before proceeding.

**How it works:**
1. Generate the 256-bit random sqlcipher key (per D2).
2. Generate a separate 256-bit recovery key.
3. Encrypt the sqlcipher key with the recovery key.
4. Store the encrypted blob in a plaintext-safe location (app data dir
   or alongside the DB file).
5. Display the recovery key to the user as a base64 or mnemonic string.
6. User must copy it or download a `.txt` file.
7. If the keychain is lost, user pastes the recovery key → app decrypts
   the blob → re-stores the sqlcipher key in the new keychain.

**Rationale:**
- One-time friction, not per-launch. Acceptable UX cost.
- The recovery key is the only thing the user needs to save. Everything
  else is reconstructable.
- This is what password managers do (Bitwarden, 1Password) and users
  understand the pattern.
- Ships in Phase 1, not Phase 2. grok-bot was right: deferring this is
  shipping a known footgun.

**Rejected alternatives:**
- Defer to Phase 2 export/import (too late — users have data by then).
- Passphrase on every launch (more friction than a one-time ceremony).
- No recovery mechanism ("accept keychain loss = data loss" — not
  acceptable for a tool people trust with their data).

---

*New decisions go below this line.*
