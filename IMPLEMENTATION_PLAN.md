# Implementation Plan

This is the sole owner of current tactical state and the active milestone. `Roadmap.md` supplies the major capability sequence; `Spec.md` supplies architectural requirements. Reconcile this plan against code before selecting work.

## Current milestone: temporal continuity and saved substreams

**State:** planned; implementation has not begun. First restore and verify the existing baseline, then implement a thin slice of Roadmap A. The architecture/document reconciliation is complete; it does not deliver temporal-stream runtime features.

**Target:** inspect source-backed changes across file and Calendar records, distinguish event time from recording time, and save an overlapping query view without moving or copying its source records.

## Baseline review

Reviewed on 2026-09-23 against `main` at `25975bd` (before this documentation change). The following inventory reports source-code evidence, not a certification that the desktop builds or each former milestone is complete.

| Area | Evidence in repository | Limits and remaining work |
| --- | --- | --- |
| Encrypted memory | `crates/wkyt-vault/src/keys.rs` and `vault.rs`: SQLCipher, key wrapping, recovery, rotation, transactional batch/cursor persistence; vault lifecycle tests. | Preserve the substrate and rerun checks before implementation. No fresh security audit is claimed. |
| Ingestion | `crates/wkyt-core/src/delta.rs`, `crates/wkyt-host/`, file and Google connector crates: bounded batches, stable identities, cursor replay, tombstones, source payload handling. | Source-specific deletion and derivation propagation need review before claiming complete history. The file import watcher is not a saved-query watcher. |
| Semantic knowledge | `crates/wkyt-core/src/item.rs`; file and Calendar connectors emit claims and evidence relationships. | Epistemic types exist, but evidence quality and temporal claims still need scrutiny; importing a scheduled event does not prove it occurred. |
| Retrieval and history | `Vault::temporal_claims_with_evidence`, `item_revisions`, and `get_entity_cluster`; claim/evidence and revision UI in `+page.svelte`. | The claim query returns live claims ordered by event timestamp; it has no project/time-range parameters or as-known-at reconstruction. The revision trigger tracks changes to properties, deletion, and validity end, not every field-only change. Cluster traversal follows explicit `same_as` relationships; it is not an automatic ambiguity-aware resolver. |
| Capability prototypes | `crates/wkyt-core/src/capability.rs`; `list_capabilities` and `invoke_capability` in `desktop/wkyt/src-tauri/src/vault_commands.rs`; a fixed query/analyze/query/report chain in the frontend. | Contracts are JSON schemas plus an approval enum, not a complete access/retention enforcement runtime. The workspace is a hard-coded composition, not a general composition engine. |
| Legacy reasoning experiment | `crates/wkyt-core/src/agent.rs`, `AgentTrace` and disagreement types, deterministic handlers in `vault_commands.rs`. | These artifacts do not establish a multi-agent runtime or enforced context bounds. The challenge handler doubts the first two claims; anomaly detection uses keywords. Fixed roles are no longer architectural requirements. Runtime/API removal is separate compatibility work, not performed by this documentation change. |
| Report prototype | `core.write_report` returns Markdown from caller-supplied claims. | The report is not persisted as a typed summary with reproducible inputs, query identity, or revision lineage. |
| Approval prototype | `RequireHuman`, pending in-memory requests, `authorize-capability` event, and `resolve_authorization`. | No durable authorization/action history, contextual lease, expiry, revocation, or rollback is established. `connector.file.write` writes caller-supplied content to a caller-supplied path; the former assertion that it only writes safe text is unsupported. Review against the plaintext invariant before using it with personal data. |
| Explicit human records | `Goal`, `Task`, `ContextEstimate`, `human_context_items`, declaration handlers, and a Human Context UI panel. | This is a declaration/display prototype, not an attention scheduler. It does not establish expiry, correction, and disable controls for inferred state. Further cognitive-state work is deferred to `TOOS.md`. |
| Build automation | `.github/workflows/ci.yml` defines desktop builds and Linux workspace tests. | Documentation-only paths do not trigger this workflow. A configured check is not evidence of a currently green result. |

### Baseline blockers and verification

Static review found several newer handlers in `desktop/wkyt/src-tauri/src/vault_commands.rs` constructing `DeltaBatch { sync_cursor, deltas }` and calling `apply_batch(connector_id, batch)`. The current types require `DeltaBatch { connector_id, deltas, cursor }` and `Vault::apply_batch(&DeltaBatch)`. Resolve this mismatch and any additional build failures before treating the prototypes as runnable.

`cargo check --workspace --offline` exited 101 with 22 compiler errors in the existing desktop backend: missing `wkyt_core::AuthorizationPolicy` re-export (E0433), outdated `DeltaBatch` fields and `apply_batch` calls (E0560/E0061), and missing `Deserialize` for `ClaimView` (E0277). No application code, dependency, persisted schema, or runtime role was changed here. Do not infer a passing build from the documentation commit.

## Next implementation slice

- [ ] Repair baseline build/API mismatches and run the required Rust and frontend checks. Keep those repairs separate from architectural expansion.
- [ ] Design the smallest encrypted history extension that preserves source and revision identity, event/recording time, corrections, and tombstones. Record migration and replay behavior in a decision before changing persisted formats.
- [ ] Add one bounded cross-source query with explicit filters, time axis, stable ordering, and an inspectable as-known-at boundary for the supported records. State coverage limits instead of claiming all system activity is reconstructed.
- [ ] Persist a revisioned saved-query definition and expose it as a live view. Prove that overlapping views reuse records and removing a view does not delete evidence.
- [ ] Verify late-arriving evidence, tied timestamps, source corrections/deletions, restart/replay, query revision, and source retention. Keep sensitive indexes and derived state encrypted or in memory.
- [ ] Demonstrate the supported file/Calendar slice and update this inventory with implementation and test evidence before marking it complete.

Broader authorization and action history remains an explicit gap until implemented. Do not connect a watcher to external effects while that boundary is unresolved. Typed summaries, future-time behavior, and deterministic watchers follow in Roadmap B; no generalized scheduler, query language, event-sourcing rewrite, local LLM, browser plugin, or WASM host is required for this first slice.

## Historical milestone reconciliation

Earlier versions of this plan marked tasks under Phases 1–5 complete. Their commits remain the historical record; this inventory replaces broad completion claims with observed scope:

- Former Phase 0 established the memory/ingestion substrate.
- Former Phase 1 added claim/evidence primitives, basic queries, item revisions, and explicit relationship traversal; full temporal reconstruction remains outstanding.
- Former Phase 2 added capability and transient-interface prototypes.
- Former Phase 3 added the now-obsolete fixed-role experiment; multiplying runtime roles is no longer a milestone.
- Former Phase 4 added an approval handshake, not complete negotiated trust.
- Former Phase 5 added declaration/display primitives, not adaptive human-context cooperation.

## Documentation verification

- Reviewed all nine documentation diffs for architectural consistency and checked status claims against source.
- Local Markdown file links and explicit heading anchors resolve; `git diff --check` passes.
- Searches found no active fixed-role/team requirement or stale current-objective section in the controlling documents. D14 retains its original rationale with an explicit D16 supersession notice.
- The baseline Rust check fails as detailed above. Workspace tests and frontend checks were not run for this prose-only change; no passing application build or runtime milestone is claimed.
