# Implementation Plan

## Recovery Assessment: Phase Ordering and CI

### Findings
- Git history shows Phase 3 Milestone 1 landed before Phase 2 Milestone 2, followed by Phase 4 and Phase 5 prototype work.
- The current Svelte frontend check and static build pass locally, so the frontend is not the immediate breakage point.
- The local default Rust toolchain was `1.87.0`, but the current lockfile resolves crates that require Rust `1.88`, so `cargo test --workspace --locked` fails before compiling the workspace under the local default toolchain.
- Full local workspace Rust testing with a newer toolchain is blocked by missing Linux system packages (`glib-2.0` and `dbus-1`) that CI installs in `.github/workflows/ci.yml`; apt installation is blocked in this container by a 403 proxy response.

### Recovery Decision
- Treat Phase 1/2 as the stabilization target and treat Phase 3/4/5 as reversible prototypes until the baseline is green.
- Make the Rust minimum explicit (`rust-version = "1.88"`) while letting CI use the current stable toolchain, so dependency MSRV is documented without pinning runners to an already-aging compiler.
- Do not advance roadmap status beyond Phase 2 until the canonical Phase 1 cross-source demo and the Phase 2 transient workspace both pass full CI.

### Recovery Checklist
- [x] Explicitly document the required Rust version in workspace metadata and keep CI on stable Rust.
- [ ] Re-run Tauri CI on runners with the documented Linux packages available.
- [ ] If CI still fails, fix compile/test errors in place rather than adding new phase work.
- [ ] After CI is green, decide whether to keep, hide, or revert Phase 3/4/5 prototype UI behind a development flag.

## Phase 1 Milestone 1: Knowledge Primitives & Temporal Synthesis

### Objective
Prove the Phase 1 conceptual model: "Can records become inspectable knowledge without losing their evidence?"

### Tasks
- [x] 1. Add claim, evidence, and provenance primitives without replacing raw items (`wkyt-core` and vault schema).
- [x] 2. Derive claims from both Calendar and file-import records. (Connector changes to map raw records to claims/entities).
- [x] 3. Implement one cross-source temporal query in the vault.
- [x] 4. Display claims beside evidence and uncertainty in the frontend viewer.
- [x] 5. Keep all derivation deterministic initially; an LLM is optional for this slice.

### Findings & Updates
- Implemented `Claim` and `Relationship` primitives. 
- Updated `vault.rs` with `temporal_claims_with_evidence`.
- Added Decision D15 to formally document the data transformation pattern.
- Frontend implemented displaying claims with their evidence bounding timestamps and epistemic state.
- Implemented Tauri IPC binding for `query_claims` to replace the frontend mock, completing the Phase 1 Milestone 1 vertical slice.

### System Invariants & Risks
- **Local authority & plaintext-at-rest**: Schema changes must use SQLite types and avoid logging sensitive data in plaintext.
- **Provenance**: Derived claims must reference their source `Item` ID.
- **Deterministic**: ID generation for claims must be stable.

### Next Steps
- Implement WASM connector sandboxing (the M5 host) or browser plugin integration as defined in the roadmap.

## Phase 2 Milestone 1: Capability Runtime and Task Interfaces

### Objective
Fulfill the Phase 2 milestone: "a demonstrable path from ingestion → knowledge/provenance → query or capability → inspectable interface over isolated framework construction."

### Tasks
- [x] 1. Define `CapabilityManifest`, `CapabilityInvocation`, and `CapabilityResult` in `wkyt-core`.
- [x] 2. Implement `list_capabilities` and `invoke_capability` in the `wkyt-vault` commands for the frontend.
- [x] 3. Wrap the temporal cross-source query (`core.query_claims`) as the first formal capability.
- [x] 4. Update the frontend with a generic capability registry interface (Preview) that can invoke capabilities and display results.

### Findings & Updates
- Created `capability.rs` in `wkyt-core` to formalize the capability contract (inputs, outputs, side-effects).
- Updated `vault_commands.rs` to expose `list_capabilities` and `invoke_capability` directly to the Svelte frontend.
- Frontend now has an inspectable capability testing UI that allows running registered capabilities and visualizing the raw JSON output.

## Phase 1 Milestone 2: Epistemic Distinctions & Entity Resolution

### Objective
Fulfill the remaining Phase 1 outcomes by adding revision history, epistemic distinctions, and ambiguity-preserving entity resolution.

### Tasks
- [x] 1. Expand `ItemKind` or Claim schema to distinguish between observation, imported assertion, inference, hypothesis, and generated suggestion.
- [x] 2. Implement temporal validity and revision history for claims in `wkyt-vault`.
- [x] 3. Implement entity resolution that preserves ambiguity rather than silently merging records.
- [x] 4. Update the frontend viewer to visualize entity clusters, epistemic states, and claim revision history.

### Findings & Updates
- Implemented `valid_to` (and `valid_to_ms` in the database) for Items to support temporal validity intervals.
- Implemented `item_revisions` table with a SQLite trigger (`item_update_revision`) that automatically saves historical state of `items` whenever `properties`, `deleted_at_ms`, or `valid_to_ms` change. This fulfills the revision history requirement for claims.
- Extracted `epistemic_state` from claim properties and displayed it on the frontend.
- Added a "View History" toggle to claims on the dashboard to query and display the `item_revisions` for a given claim.
- Implemented `get_entity_cluster` in `wkyt-vault` which uses a recursive CTE to follow `same_as` relationships and aggregate entity clusters, solving the entity resolution requirement while preserving underlying ambiguity.

## Phase 3 Milestone 1: Deterministic Agent Abstractions

### Objective
Fulfill the Phase 3 milestone: "A temporal question is answered by a small team of specialized agents, with disagreement and evidence visible rather than collapsed into one unexplained response." We implement this deterministically first before introducing LLM inference, ensuring the abstractions are sound.

### Tasks
- [x] 1. Define `AgentManifest` and `AgentRole` (Planner, Specialist, Skeptic, Verifier) in `wkyt-core`.
- [x] 2. Extend `ItemKind` and `EpistemicType` in `wkyt-core` to natively represent Agent Traces and Disagreements.
- [x] 3. Create a deterministic "Skeptic" agent invocation capability that evaluates and challenges existing claims.
- [x] 4. Update the Svelte frontend UI to explicitly render branching hypothesis paths or conflicting claims attributed to specific agents.

### System Invariants & Risks
- **Provenance**: Agent-derived claims must link back to their originating Agent ID and the specific execution trace/evidence they evaluated.
- **Trust**: Agents only receive data via explicit capability bounds.
- **Plaintext-at-rest**: Agent execution outputs and intermediate reasoning remain strictly in the encrypted vault or memory; no plaintext logging of agent scratchpads.
- **Migration**: UUIDv5 `source_id`s for agents must be structured to be deterministic and replay-safe to prevent orphaned claims.

## Phase 2 Milestone 2: Capability Composition & Transient Workspaces

### Objective
Fulfill the Phase 2 milestone requirement: `"Build a dashboard from these logs, explain the anomaly, and prepare a report" composes retrieval, analysis, visualization, and writing capabilities into one transient workspace.`

### Tasks
- [x] 1. Implement a deterministic `agent.anomaly_detector` capability that reads claims and flags any containing terms like "error", "fail", or "anomaly" as a new Hypothesis claim.
- [x] 2. Implement a `core.write_report` capability that takes claims as input and generates a summarized markdown report.
- [x] 3. Update the frontend UI with a "Transient Task Workspace" that sequentially chains `core.query_claims` -> `agent.anomaly_detector` -> `core.query_claims` -> `core.write_report`.
- [x] 4. Ensure provenance is preserved by saving analyzer-generated claims into the encrypted vault before the report is written.

### Findings & Updates
- Implemented `agent.anomaly_detector` and `core.write_report` in `vault_commands.rs`.
- Created a deterministic chain that satisfies the composition requirement without relying on a bulky local LLM.
- Updated `+page.svelte` to invoke these capabilities sequentially, proving the UI can orchestrate complex tasks across multiple capabilities and visually present the result.

## Phase 4 Milestone 1: Negotiated Trust and Safe Action

### Objective
Prove the Phase 4 conceptual model: "The system proposes and, after appropriate authorization, performs a bounded external action while showing exactly what accessed which data and what changed."

### Tasks
- [x] 1. Refactor `wkyt-core` `CapabilityManifest` to use `authorization_policy` instead of a simple `side_effects` bool.
- [x] 2. Implement an externally acting capability (`connector.file.write`) as a proof-of-concept.
- [x] 3. Refactor `invoke_capability` in `wkyt-vault` to detect `RequireHuman` policies, pause execution via `tokio::sync::oneshot`, and emit an `authorize-capability` event to the frontend.
- [x] 4. Update the Svelte frontend to listen for authorization events, present a dry-run explanation to the user, and allow them to approve or deny.
- [x] 5. Add `resolve_authorization` Tauri command to resume execution upon user approval.

### System Invariants & Risks
- **Local authority**: User explicitly grants authorization for side effects.
- **Plaintext-at-rest**: Capability payloads must not contain sensitive unencrypted data in logs. The proof-of-concept file write creates a safe text file.
- **Trust & Provenance**: Human approval is explicitly required for mutative external actions.

## Phase 5 Milestone 1: Human Context as Cooperation

### Objective
Fulfill the Phase 5 milestone: "explicit goals, active tasks, commitments, and interruption state; optional and correctable estimates of expertise, confidence, fatigue, interruptibility, and working-memory load; confidence, provenance, expiry, and disable controls for all inferred human state;"

### Tasks
- [x] 1. Extend `ItemKind` in `wkyt-core` to include `Goal`, `Task`, and `ContextEstimate`.
- [x] 2. Implement `human_context_items` vault query to retrieve these specific items.
- [x] 3. Expose new capabilities (`core.declare_goal`, `core.declare_task`, `core.update_context_estimate`) in `vault_commands.rs`.
- [x] 4. Update the Svelte frontend to fetch and display a dedicated "Human Context" panel, allowing the user to inspect and declare their current goals, tasks, and cognitive states.

### System Invariants & Risks
- **Provenance & Trust**: Human context items are first-class primitives in the vault and carry timestamps and explicit provenance. Updates via capabilities are tracked.
- **Local authority**: Goals and estimates are explicitly declared and visible to the user, allowing for correction and inspection.
- **Plaintext-at-rest**: Human context data is stored in the encrypted SQLite vault just like any other claim or event.
