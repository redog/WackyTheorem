# Implementation Plan

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
