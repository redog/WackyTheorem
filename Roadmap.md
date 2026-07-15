<<<<<<< Updated upstream
# Phase 1: Core Infrastructure & Unified Data Vault — COMPLETED
=======
# WackyTheorem Roadmap
>>>>>>> Stashed changes

The roadmap measures progress by changes in the computing model, not only by feature count. Each phase should leave a usable vertical slice and preserve all invariants in `Spec.md`.

## Phase 0: Encrypted Memory Substrate — COMPLETE

**Question answered:** Can the system acquire personal data, preserve source identity, survive interruption, and store it without plaintext leakage?

Delivered:

- encrypted SQLCipher vault with KEK/DEK lifecycle and recovery;
- replay-safe, bounded ingestion;
- opaque cursors, tombstones, and deterministic identity;
- local file and Google Calendar connectors;
- minimal record viewer;
- CI and end-to-end tests.

<<<<<<< Updated upstream
Done:
- [x] Cargo workspace: `wkyt-core`, `wkyt-vault`, `wkyt-broker`, `wkyt-connector-file`, `wkyt-host` + Tauri app (M1)
- [x] Encrypted vault: sqlcipher, KEK/DEK keychain hierarchy, recovery-key ceremony with forced verification, crash-safe DEK rotation (M2, D8/D12)
- [x] Connector contract: streaming batches, opaque cursors, error taxonomy, tombstones, deterministic UUIDv5 identity (D13)
- [x] Pipeline: bounded in-process bus (D11), transactional batch+cursor apply, ack-after-commit, crash-replay without duplicates (M3)
- [x] File-importer connector + import-folder watcher (M4)
- [x] Viewer dashboard (Spec DoD #7) and first-run ceremony UI (with downloadable .txt recovery key)
- [x] CI green on Linux/macOS/Windows, `cargo test` enforced (Spec DoD #8)
- [x] Google OAuth 2.0 PKCE flow, tokens in OS keychain (D3/D5 — Spec DoD #2–4)
- [x] Google Calendar connector, ≥30 days of events (D4 — Spec DoD #5)
- [x] Headless-Linux keyring fallback: passphrase + Argon2id (D2)

# Phase 2: Personal LLM & Temporal Graph — IN PROGRESS
=======
The existing desktop application is the bootstrap environment for later phases.

## Phase 1: Provenance-Bearing LifeGraph — NEXT

**Question answered:** Can records become inspectable knowledge without losing their evidence?

Outcomes:
>>>>>>> Stashed changes

- explicit entity, event, claim, relationship, and evidence primitives;
- temporal validity and revision history;
- source-to-claim provenance chains;
- distinction between observation, imported assertion, inference, hypothesis, and generated suggestion;
- entity resolution that preserves ambiguity rather than silently merging;
- queries that cross at least two connectors.

Milestone:

> The system explains what changed about a selected project or person during a time range and shows the evidence for every material claim.

<<<<<<< Updated upstream
Planned for Phase 2:
- [ ] Implement WASM connector sandboxing (the M5 host).
- [ ] Implement the browser plugin for data ingestion.
- [ ] Integrate a local LLM and embedding pipeline.
- [ ] Build the temporal graph query engine.

# Phase 3: Action API & Mobile Foundation
=======
## Phase 2: Capability Runtime and Task Interfaces
>>>>>>> Stashed changes

**Question answered:** Can the environment solve a task without requiring the user to choose an owning application?

Outcomes:

- capability manifests with typed inputs, outputs, authority, side effects, and retention;
- a capability registry and composition runtime;
- temporary task-oriented interfaces assembled from reusable views;
- conventional tools and applications wrapped as engines behind capability contracts;
- inspectable plans and execution traces.

Milestone:

> “Build a dashboard from these logs, explain the anomaly, and prepare a report” composes retrieval, analysis, visualization, and writing capabilities into one transient workspace.

## Phase 3: Distributed Cognition

**Question answered:** Can several narrow agents cooperate, disagree, verify, and expose uncertainty?

Outcomes:

- agent manifests and bounded context grants;
- planner, domain specialist, skeptic, and verifier roles;
- provenance for agent conclusions and transformations;
- confidence, conflicting evidence, and unresolved assumptions visible in the UI;
- local-model support with deterministic tools preferred where suitable;
- no requirement for one omniscient assistant persona.

Milestone:

> A temporal question is answered by a small team of specialized agents, with disagreement and evidence visible rather than collapsed into one unexplained response.

## Phase 4: Negotiated Trust and Safe Action

**Question answered:** Can the system act while preserving user agency and understandable security?

Outcomes:

- contextual, revocable capability leases;
- purpose, duration, retention, and side-effect declarations;
- approval policies proportional to action risk;
- dry runs, rollback plans, and audit trails;
- sandboxed third-party connectors and capabilities.

Milestone:

> The system proposes and, after appropriate authorization, performs a bounded external action while showing exactly what accessed which data and what changed.

## Phase 5: Human Context as Cooperation

**Question answered:** Can the environment coordinate with the human rather than merely respond to prompts?

Outcomes:

- explicit goals, active tasks, commitments, and interruption state;
- optional and correctable estimates of expertise, confidence, fatigue, interruptibility, and working-memory load;
- confidence, provenance, expiry, and disable controls for all inferred human state;
- attention-aware scheduling and interruption negotiation;
- no hidden behavioral manipulation.

Milestone:

> The environment delays, summarizes, or surfaces work according to the user's declared goals and visible context model, and the user can inspect and correct every assumption.

## Phase 6: Reversible Personal Computing

**Question answered:** Can knowledge, actions, interfaces, and system state be explored and reversed over time?

Outcomes:

- time-travel views for claims, decisions, permissions, and transformations;
- durable undo and compensating actions where true reversal is impossible;
- branching hypotheses and alternative plans;
- exportable open schemas and user-controlled migration;
- multi-device operation without surrendering local authority.

Milestone:

> The user can inspect how a belief or outcome evolved, restore a prior state, or branch an alternative plan with its full provenance intact.

## Immediate implementation slice

Build the smallest end-to-end proof of Phase 1:

1. Add claim/evidence/provenance primitives without replacing raw items.
2. Derive claims from both Calendar and file-import records.
3. Implement one cross-source temporal query.
4. Display claims beside evidence and uncertainty.
5. Keep all derivation deterministic initially; an LLM is optional for this slice.

## Open questions for operator

- Which cross-source question should become the canonical Phase 1 demo?
- Should sandboxed WASM connectors be pulled into Phase 1 because capability isolation is foundational, or remain in Phase 4?
- What is the minimum open LifeGraph schema worth stabilizing for third-party experimentation?
