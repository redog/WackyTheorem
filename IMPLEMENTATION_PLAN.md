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
- Move towards Phase 2: Personal LLM integration and the temporal graph query engine.
- Implement WASM connector sandboxing (the M5 host) or browser plugin integration as defined in the roadmap.
