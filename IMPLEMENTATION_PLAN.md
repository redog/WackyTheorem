# Implementation Plan

This is the sole owner of current tactical state and the active milestone. `Roadmap.md` supplies the major capability sequence; `Spec.md` supplies architectural requirements. Reconcile this plan against code before selecting work.

## Current milestone: restore a verified build baseline

**State:** in progress. Repair the existing Rust/frontend failures and verify local checks plus a fresh CI run before resuming Roadmap A. The architecture reconciliation is complete; temporal-stream implementation remains planned.

**Target:** a reproducible build and test baseline, with precise failure records and instructions that prevent an agent from advancing while checks fail. Keep repairs within existing contracts; no schema migration, new feature, or change to authorization policy is intended.

The trigger-only commit `6aac416` started CI run `35849129769`. It also makes build-control-document changes trigger CI.

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
| Build automation | `.github/workflows/ci.yml` defines desktop builds and Linux workspace tests. | Build-control documents now trigger this workflow. Frontend and Rust checks run before packaging; Linux runs workspace tests. A configured check is not evidence of a currently green result. |

### Original failures and repair scope

Static review found several newer handlers in `desktop/wkyt/src-tauri/src/vault_commands.rs` constructing `DeltaBatch { sync_cursor, deltas }` and calling `apply_batch(connector_id, batch)`. The current types require `DeltaBatch { connector_id, deltas, cursor }` and `Vault::apply_batch(&DeltaBatch)`. Resolve this mismatch and any additional build failures before treating the prototypes as runnable.

The original `cargo check --workspace --offline` exited 101 with 22 compiler errors: missing `wkyt_core::AuthorizationPolicy` re-export (E0433), outdated batch fields/calls (E0560/E0061), and missing `Deserialize` for `ClaimView` (E0277). The diagnostic CI run reproduced these failures on both Linux runners.

The repair exports the existing policy type, makes report claim/evidence payloads deserializable, and uses the current batch contract in all five affected handlers. Local capability writes use `cursor: None` because they do not advance an external connector checkpoint. No schema, encryption, authorization policy, or dependency version is changed.

CI now uses Node.js 22 and `npm ci`, validates frontend types and the Rust workspace before packaging, and runs Linux tests before packaging. The unnecessary macOS Homebrew GTK installation is removed; GTK is a Linux backend dependency. Both Linux runners and the Windows/macOS jobs are retained.

## Recovery verification

- `cargo check --workspace --locked`: passed locally.
- `npm ci`, `npm run check`, and `npm run build`: passed locally; Svelte reports zero errors and warnings.
- `cargo test --workspace --locked`: passed locally (67 tests). The initial sandbox run could not bind local mock HTTP ports; the rerun with loopback access passed.
- Workflow YAML and diff checks passed. CI on the repair commit remains required before recovery is complete.
- `npm ci` reports five pre-existing dependency audit findings; dependency remediation is separate from this build repair.

## Next implementation slice (after build recovery)

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

## Documentation reconciliation

The preceding architecture-only commit reconciled nine control documents and checked local links/diffs. This recovery pass makes baseline repair the active milestone and requires exact-commit CI evidence before the coding loop resumes feature work. Roadmap A remains planned; passing builds do not certify the unmet architecture guarantees in the inventory above.
