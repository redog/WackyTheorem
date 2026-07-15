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

**Panel input (gemini-bot):** gemini-bot's crate survey confirmed
`rusqlite` with sqlcipher feature is the de facto standard for encrypted
SQLite in Rust. Recommended `features = ["sqlcipher"]` (system lib).
We're using `bundled-sqlcipher` instead to avoid a system dependency in
CI — same encryption, self-contained build.

**Rejected alternatives:**
- DuckDB + application-layer encryption (complex, Spec says no).
- `libsqlite3-sys` with system sqlcipher (works, but `bundled-sqlcipher`
  is simpler for CI and cross-platform builds).
- `rusqlite` with `features = ["sqlcipher"]` non-bundled (gemini-bot's
  suggestion — valid, but adds a system library dependency that
  complicates CI and first-time contributor builds).

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
- claude-bot analyzed four options in detail: (A) OS keychain random key,
  (B) user passphrase → KDF, (C) machine ID derivation ("security theater
  — don't do this"), (D) Tauri Stronghold ("over-engineered for this use
  case"). Recommended Option A with explicit trust documentation.
- grok-bot flagged Linux keychain reliability as a real risk — GNOME
  Keyring / KWallet may not be present on headless or minimal installs.
- claude-bot recommended CI testing on multiple Linux environments
  and graceful degradation. Specific CI matrix: Ubuntu with GNOME
  (keyring available), Ubuntu headless (keyring absent — verify graceful
  fallback). `libsecret-1-dev` required as CI dependency.
- groq-bot concurred with keyring recommendation.
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
- Hybrid: keychain stores key AND key is additionally encrypted with a
  user passphrase (grok-bot suggested exploring this middle path —
  stronger than keychain-only, but adds per-launch friction. D8's
  recovery key gives us the safety net without the daily UX cost).

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
5. Store tokens in keyring (D3).
6. Shut down the temporary server.

**Rationale:**
- This is the standard PKCE flow for desktop/native apps per Google's
  docs and RFC 7636.
- No client secret needed in the binary — PKCE replaces it.
  *Update (2026-06-22):* While the binary does not compile in a client secret originally, Google's OAuth token endpoint strictly requires the `client_secret` parameter to exchange the authorization code for Desktop Client ID credentials. Since Google officially considers the Desktop client secret to be public and not a true secret, Eric explicitly authorized embedding this client secret in the binary at compile time, overriding the "no secrets in the binary" constraint for this specific case. We resolve this by reading `WKYT_GOOGLE_CLIENT_SECRET` at compile time via `option_env!`.
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
to store the tokens, and keyring setup is part of M1's "secret store"
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

## D9: Defense-in-depth measures

**Date:** 2025-06-05
**Status:** Decided
**Context:** claude-bot recommended layered security beyond just
encryption. Even if the DB is encrypted, good practice says: minimize
attack surface at every layer.

**Decision:** Implement three defense-in-depth measures in Phase 1:

1. **File permissions:** Set DB file to 0600 (owner read/write only)
   on creation. Prevent other users/processes from reading the encrypted
   file even if they have filesystem access.
2. **Tauri security headers:** Use Tauri's CSP and security
   configuration to restrict what the webview can do. No inline scripts,
   no external resource loading beyond what's needed.
3. **Network restriction:** The app should only make outbound requests
   to Google APIs (OAuth + Calendar). No analytics, no telemetry, no
   other endpoints. Document allowed domains.

**Rationale:**
- Defense-in-depth is standard practice. Encryption is one layer; file
  permissions, CSP, and network policy are others.
- These are low-effort, high-value mitigations.
- claude-bot specifically recommended all three as part of the
  architecture decision.

---

## D10: Vault engine re-evaluation — DuckDB 1.4 native encryption investigated, SQLite/sqlcipher reaffirmed

**Date:** 2026-06-10
**Status:** Decided (reaffirms D1)
**Context:** A proposed architecture revision suggested returning to DuckDB
as the vault engine, citing DuckDB 1.4's new native encryption. D1 rejected
DuckDB because it had "no equivalent" to sqlcipher — that rationale is now
stale and needed re-examination. The critical question (flagged as a
potential blocker): are DuckDB's temporary spill files written to disk in
plaintext during large out-of-core operations?

**Investigation findings (T0.1):**
- DuckDB 1.4.0 "Andium" LTS (2025-09-16) introduced database encryption
  using AES-256-GCM (AES-256-CTR also available, not recommended — no
  integrity tag). Key supplied via `ATTACH ... (ENCRYPTION_KEY '...')`.
- **Temp-spill is NOT a blocker:** per DuckDB's official "Data-at-Rest
  Encryption in DuckDB" post (2025-11-19), encryption covers the main
  database file, the WAL, *and* temporary files. Temp files are encrypted
  automatically with internally generated ephemeral keys.
- **However:** GHSA-vmp8-hg63-v2hp / CVE-2025-64429 (fixed in 1.4.2,
  2025-11-12) disclosed four flaws in the initial crypto implementation:
  (1) fallback to a non-cryptographic RNG (pcg32) for key/IV generation,
  (2) key zeroization via `std::memset` that compilers may optimize away,
  (3) a GCM→CTR header downgrade attack bypassing integrity checks,
  (4) unchecked OpenSSL `RAND_bytes` return value. Any DuckDB adoption
  would require >= 1.4.2.

**Decision:** Stay on SQLite + sqlcipher per D1 and the Spec. DuckDB's
encryption is no longer missing, but it is months old and has already had
a multi-flaw advisory; sqlcipher has ~15 years of production scrutiny.
The Spec also hard-constrains storage to SQLite, and nothing in Phase 1's
workload (single user, modest volumes, point lookups) needs an OLAP
engine. Re-evaluate only if Phase 2 analytics genuinely outgrow SQLite.

**Follow-up for M2:** verified that the `bundled-sqlcipher` build keeps
  SQLite temp storage in memory (`SQLITE_TEMP_STORE`/`PRAGMA temp_store`),
  with an integration test `temp_store_is_memory` validating that no
  plaintext intermediate temp files are written to disk.

**Rejected alternatives:**
- DuckDB >= 1.4.2 with native encryption (viable on the merits, but
  contradicts the Spec constraint and adds crypto-maturity risk for no
  Phase 1 benefit).

---

## D11: Ingestion transport — in-process bus, not NATS JetStream

**Date:** 2026-06-10
**Status:** Decided
**Context:** A proposed architecture revision called for connectors to
publish protobuf deltas to an embedded NATS JetStream broker, consumed by
a vault writer.

**Decision:** No broker. Connector deltas flow over an in-process,
bounded async channel behind a small `Bus` trait. Durability and
exactly-once-effect come from the vault, not the transport: each batch is
applied in one transaction together with the connector's sync cursor, and
writes are idempotent (D13), so any crash is recovered by resuming from
the last committed cursor and replaying.

**Why NATS was rejected:**
1. **There is no embeddable NATS server in Rust.** The server is Go, so
   "embedded" really means a sidecar process: lifecycle supervision, a
   loopback TCP port, and broker credentials — a new secret to manage
   that exists only to protect the broker we introduced.
2. **JetStream's file store persists stream data in plaintext**, directly
   violating the Spec's non-negotiable "nothing in plaintext on disk."
   Memory-only streams avoid that but forfeit the durability that
   justified JetStream in the first place.
3. **Wrong scale.** This is a single-producer, single-consumer,
   single-process pipeline. A bounded `tokio::mpsc` channel provides the
   same backpressure with zero extra processes, ports, or keys.

**Insurance:** the `Bus` trait keeps the transport swappable. If a future
phase has a real multi-process need (mobile sync daemon, external
connector processes), a broker implementation can be added behind the
same interface without touching connectors or the vault.

**Rejected alternatives:**
- NATS JetStream sidecar (above).
- Durable queue table in the encrypted DB as the transport (workable,
  but redundant: transactional batch-apply + cursors already provide
  crash recovery; a queue table adds write amplification for no gain).

---

## D12: Key hierarchy — KEK/DEK split anchored to the OS keychain; TPM sealing rejected

**Date:** 2026-06-10
**Status:** Decided (refines D2, integrates D8)
**Context:** A proposed architecture revision called for sealing the
database key in a TPM/secure enclave. Separately, D2's current design has
the sqlcipher key live directly in the keychain with nothing between the
secret store and the data — which makes key rotation and recovery
needlessly expensive.

**Decision:** Two-tier key hierarchy:
- **DEK (data encryption key):** random 256-bit, generated once; this is
  what sqlcipher receives via `PRAGMA key`. It exists unwrapped only in
  process memory and is stored on disk solely as wrapped blobs.
- **KEK (key encryption key):** random 256-bit, stored in the OS keychain
  via `keyring` (per D2). Wraps the DEK using an AEAD (XChaCha20-Poly1305
  or AES-256-GCM); the wrapped-DEK blob (versioned, authenticated) lives
  in the app data dir.
- **Recovery key (D8) becomes a second KEK:** the recovery ceremony wraps
  the *same* DEK under the recovery key, producing a second blob. Either
  wrapper unlocks the vault; losing the keychain is recoverable without
  re-encrypting anything.

**Cold-start flow:** app launch → OS login session has already unlocked
the keychain → KEK fetched silently → DEK unwrapped in memory →
`PRAGMA key` → UI renders. Zero user interaction in the common case; the
security boundary is the OS login, stated honestly.

**What the split buys:**
- KEK rotation (keychain migration, future passphrase upgrade) re-wraps
  one 32-byte blob instead of re-encrypting the whole database.
- Recovery and keychain become symmetric wrappers — one mechanism, not two.
- `PRAGMA rekey` remains available for true DEK rotation after suspected
  compromise, as a separate, rarer operation.

**Why TPM sealing was rejected:**
1. **PCR fragility = data-loss footgun.** Sealing to measured-boot PCRs
   means a firmware update, Secure Boot toggle, or kernel upgrade can make
   the key permanently unsealable — routine maintenance becomes a
   data-loss event unless recovery exists anyway (so the TPM adds risk,
   not protection, relative to the keychain).
2. **No app isolation on Linux regardless.** Any process in the user's
   session can query the Secret Service, and absent an auth-value policy,
   any process with TPM access can request an unseal. The realistic threat
   model (stolen disk, other OS users, backup leakage — not malware
   running as the user) is covered equally well by the keychain.
3. **Platform variance.** TPM2 on Linux, Secure Enclave on macOS, nothing
   portable between them — large implementation surface for Phase 1.

A TPM/enclave may return later as an *optional additional* wrapper for
the same DEK (a third blob), hardware-binding without becoming a single
point of failure.

**Amendment to D8:** the recovery ceremony must *verify*, not just ask
for acknowledgment — the user re-enters (or re-pastes) the recovery key
before the app proceeds. An unverified "I saved it" click is how users
discover at restore time that they saved the wrong thing.

**Memory hygiene:** DEK/KEK buffers are zeroized on drop (`zeroize`),
and core dumps are disabled for the process via `libc::setrlimit` (setting `RLIMIT_CORE` to 0) on platforms that allow it.
Documented limitation: the key and decrypted pages necessarily exist in
process RAM while the app runs.

---

## D13: Deterministic item identity — UUIDv5 over (connector_id, source_id)

**Date:** 2026-06-10
**Status:** Decided
**Context:** `Item::new` currently assigns `id = Uuid::new_v4()` on every
construction, while the storage upsert conflicts on `id`. Consequence:
re-syncing the same source record produces a fresh UUID every time, the
`ON CONFLICT` clause never fires, and **every re-sync duplicates every
item**. Separately, `Item::new` defaults `timestamp` to `Utc::now()`,
silently corrupting the temporal axis (the thing Phase 2 queries depend
on) whenever a connector forgets to overwrite it.

**Decision:**
1. `Item.id` is derived deterministically:
   `Uuid::new_v5(WKYT_NAMESPACE, connector_id + 0x1F + source_id)`, where
   `WKYT_NAMESPACE` is a fixed project namespace UUID generated once and
   committed as a constant, and `0x1F` (ASCII unit separator) prevents
   concatenation collisions (`("a|b", "c")` vs `("a", "b|c")`).
2. The vault additionally enforces `UNIQUE(connector_id, source_id)` as
   a belt-and-braces constraint independent of ID derivation.
3. `Item::new` takes the event timestamp as a **required parameter** —
   no `Utc::now()` default for `timestamp`. (`ingested_at` keeps the
   `now()` default; that one really is ingestion time.)

**Rationale:** idempotent writes are the foundation D11 stands on (crash
recovery by replay) and the fix for the duplication bug. Same input item
in, same row out, no matter how many times it's synced.

**Rejected alternatives:**
- Random UUID + `ON CONFLICT (connector_id, source_id)` upsert (works,
  but then `id` is unstable across reinstalls/re-ingests, and anything
  referencing items by `id` — Phase 2 graph edges — breaks).
- Natural composite primary key `(connector_id, source_id)` with no UUID
  (simpler, but wide foreign keys everywhere and leaks source IDs into
  every referencing table).

---

## Amendment to D1 — `bundled-sqlcipher-vendored-openssl` (approved by Eric)

**Date:** 2026-06-10
**Status:** Decided (amends D1; approved at M2 security review)
**Context:** D1 chose rusqlite's `bundled-sqlcipher` to avoid a system C
library. That feature still links the *system* OpenSSL for sqlcipher's
crypto, which breaks Spec DoD #1 ("clean Ubuntu machine, no manual setup
beyond rustup and npm install") on machines without `libssl-dev`.

**Decision:** Use `features = ["bundled-sqlcipher-vendored-openssl"]`,
which compiles sqlcipher *and* its OpenSSL from source. Slower cold
builds; zero system dependencies; same encryption.

**Rejected alternatives:**
- `bundled-sqlcipher` + system OpenSSL (breaks the clean-machine DoD).
- sqlcipher's NSS/CommonCrypto providers (platform-divergent crypto).

---

*New decisions go below this line.*

---

## D14: Product model — cognitive operating environment, not personal assistant application

**Date:** 2026-07-15
**Status:** Decided (architectural direction)

**Context:** The initial specification successfully drove construction of an encrypted desktop ingestion application. That substrate is useful, but its wording caused autonomous builders to optimize toward a conventional personal-data assistant: connectors, records, dashboard, then one LLM chat surface.

The intended system is broader. WackyTheorem is exploring a computing model where knowledge, provenance, intent, capabilities, specialized agents, negotiated trust, and human context replace applications and files as the primary user abstractions.

**Decision:** Adopt `VISION.md` as the project's north-star document and redefine the current desktop application as the bootstrap memory substrate.

Future work should preferentially:

- transform records into provenance-bearing entities, events, claims, relationships, and evidence;
- model reusable operations as capabilities rather than application-owned features;
- compose multiple narrow agents and deterministic tools rather than centering one assistant persona;
- make uncertainty, disagreement, trust, plans, and side effects inspectable;
- treat files and existing applications as compatibility engines and views;
- model human context only as optional, uncertain, correctable state used for cooperation.

**Consequences:**

- `Spec.md` now separates system invariants from current reference implementation.
- `Roadmap.md` advances by computing-model milestones rather than application feature buckets.
- `agents.md` instructs coding agents to make reversible decisions under ordinary ambiguity instead of stopping by default.
- `PROMPT_build.md` selects vertical slices that connect ingestion, provenance, capability composition, and inspectable interfaces.
- SQLite, SQLCipher, Rust, Tauri, UUIDv5, and Tokio remain current implementation choices, not definitions of the product.

**Rejected alternatives:**

- Continue describing the product primarily as a personal data assistant. This would keep future loops converging on “ChatGPT over an encrypted database.”
- Call the project a new operating system without qualification. The term suggests a kernel and hardware resource manager and obscures the intended shift in personal-computing abstractions.
- Remove conventional files and applications entirely. They remain necessary for compatibility, interchange, implementation, and inspection; they simply cease to be the canonical user model.

---

## D15: Data transformation — records map to Claims and Relationships

**Date:** 2026-07-15
**Status:** Decided (Phase 1 Milestone 1)
**Context:** Connectors ingest raw facts (Files, Calendar Events). To support higher-level temporal synthesis and the cognitive operating environment vision, the system needs abstractions for facts that might be derived, contradicted, or updated, rather than treating raw records as ground truth.

**Decision:** Connectors now emit a `Claim` and a `Relationship` for each ingested raw item (Event or File), in addition to the raw item itself.
- **Claim:** Represents a synthesis or assertion (e.g., "Event took place" or "File exists").
- **Relationship:** Connects the `Claim` to its evidence (the original `Event` or `File`).

The vault now supports a cross-source temporal query (`temporal_claims_with_evidence`) that returns claims joined with their underlying evidence.

**Rationale:**
- Preserves provenance (the claim traces back to the raw source record).
- Allows the system to hold conflicting claims from different sources without overwriting raw data.
- Begins the shift from "database of records" to "knowledge graph of claims and evidence" (vision alignment).

**Rejected alternatives:**
- Storing claims merely as properties on the original item (mixes raw source data with derived knowledge; prevents multiple sources from corroborating the same claim).
- Modifying `delta.proto` to define Claims as a separate top-level message (can just use `ItemKind::Claim` since they share the same durability and sync characteristics).
