# Implementation Plan

This is the sole owner of current tactical state and the active milestone. `Roadmap.md` supplies the major capability sequence; `Spec.md` supplies architectural requirements. Reconcile this plan against code before selecting work.

## Current milestone: connector deletion consistency (Roadmap A)

**State:** connector repair implemented and local checks/desktop acceptance passed; exact-commit CI remains the final verification gate. Starting baseline: `75eba3a`. [CI run 36048436691](https://github.com/redog/WackyTheorem/actions/runs/36048436691) passed on all four platforms, including the Ubuntu release-binary desktop acceptance gate. D18 was reviewed and merged by Eric in PR #35.

**Target:** when an incremental file deletion or Calendar cancellation is observed, soft-delete the connector-generated source, claim, and evidence relationship in one batch. Verify current queries, historical views, replay, restoration, and unrelated-source retention. Keep the change limited to these known connector derivations.

**Risks and boundaries:** no schema, encryption, authentication, external-action, or destructive migration changes. Retained item versions remain encrypted. This does not retroactively repair orphaned derivations from deletions already consumed by older connectors, infer missing records during full resync, or implement a general graph cascade. Pinned derivation references, broader action history, and retention/erasure remain future work. See D19.

## Baseline review

The architecture inventory was reviewed on 2026-09-23 against `main` at `25975bd`, then supplemented by the build recovery verified at `46943f5`. Passing build checks establish a runnable baseline, not completion of the broader architectural guarantees below.

| Area | Evidence in repository | Limits and remaining work |
| --- | --- | --- |
| Encrypted memory | `crates/wkyt-vault/src/keys.rs` and `vault.rs`: SQLCipher, key wrapping, recovery, rotation, transactional batch/cursor persistence; vault lifecycle tests. | Preserve the substrate and rerun checks before implementation. No fresh security audit is claimed. |
| Ingestion | `crates/wkyt-core/src/delta.rs`, `crates/wkyt-host/`, file and Google connector crates: bounded batches, stable identities, cursor replay, tombstones, source payload handling. | Both connectors now emit source/claim/link tombstones together for observed deletions; historical orphan cleanup and full-resync reconciliation remain gaps. The file import watcher is not a saved-query watcher. |
| Semantic knowledge | `crates/wkyt-core/src/item.rs`; file and Calendar connectors emit claims and evidence relationships. | Epistemic types exist, but evidence quality and temporal claims still need scrutiny; importing a scheduled event does not prove it occurred. |
| Item history and saved views | `history.rs`, `substreams.rs`, and `StreamPanel.svelte`: encrypted baseline and batch snapshots, cross-source filtered queries, live/pinned saved definitions, five-second live refresh, source inspection. | Coverage begins at upgrade; only writes through `apply_batch` are captured. Results are capped at 200 (UI: 50). No authorization/action audit, erasure policy, or pinned derivation chain is established. |
| Legacy retrieval | `Vault::temporal_claims_with_evidence`, `item_revisions`, and `get_entity_cluster`; claim/evidence and revision UI in `+page.svelte`. | The claim query returns live claims ordered by event timestamp; it has no project/time-range parameters or as-known-at reconstruction. The revision trigger tracks changes to properties, deletion, and validity end, not every field-only change. Cluster traversal follows explicit `same_as` relationships; it is not an automatic ambiguity-aware resolver. |
| Capability prototypes | `crates/wkyt-core/src/capability.rs`; `list_capabilities` and `invoke_capability` in `desktop/wkyt/src-tauri/src/vault_commands.rs`; a fixed query/analyze/query/report chain in the frontend. | Contracts are JSON schemas plus an approval enum, not a complete access/retention enforcement runtime. The workspace is a hard-coded composition, not a general composition engine. |
| Legacy reasoning experiment | `crates/wkyt-core/src/agent.rs`, `AgentTrace` and disagreement types, deterministic handlers in `vault_commands.rs`. | These artifacts do not establish a multi-agent runtime or enforced context bounds. The challenge handler doubts the first two claims; anomaly detection uses keywords. Fixed roles are no longer architectural requirements. Runtime/API removal remains separate compatibility work; this recovery retains the existing types and handlers. |
| Report prototype | `core.write_report` returns Markdown from caller-supplied claims. | The report is not persisted as a typed summary with reproducible inputs, query identity, or revision lineage. |
| Approval prototype | `RequireHuman`, pending in-memory requests, `authorize-capability` event, and `resolve_authorization`. | No durable authorization/action history, contextual lease, expiry, revocation, or rollback is established. `connector.file.write` writes caller-supplied content to a caller-supplied path; the former assertion that it only writes safe text is unsupported. Review against the plaintext invariant before using it with personal data. |
| Explicit human records | `Goal`, `Task`, `ContextEstimate`, `human_context_items`, declaration handlers, and a Human Context UI panel. | This is a declaration/display prototype, not an attention scheduler. It does not establish expiry, correction, and disable controls for inferred state. Further cognitive-state work is deferred to `TOOS.md`. |
| Build automation | `.github/workflows/ci.yml` defines desktop builds and Linux workspace tests. | Build-control documents now trigger this workflow. Frontend and Rust checks run before packaging; Linux runs workspace tests. A configured check is not evidence of a currently green result. |

### Original failures and repair scope

Static review found several newer handlers in `desktop/wkyt/src-tauri/src/vault_commands.rs` constructing `DeltaBatch { sync_cursor, deltas }` and calling `apply_batch(connector_id, batch)`. The current types require `DeltaBatch { connector_id, deltas, cursor }` and `Vault::apply_batch(&DeltaBatch)`. The repair at `46943f5` reconciled these callers with the current contract.

The original `cargo check --workspace --offline` exited 101 with 22 compiler errors: missing `wkyt_core::AuthorizationPolicy` re-export (E0433), outdated batch fields/calls (E0560/E0061), and missing `Deserialize` for `ClaimView` (E0277). The diagnostic CI run reproduced these failures on both Linux runners.

The repair exports the existing policy type, makes report claim/evidence payloads deserializable, and uses the current batch contract in all five affected handlers. Local capability writes use `cursor: None` because they do not advance an external connector checkpoint. No schema, encryption, authorization policy, or dependency version is changed.

CI now uses Node.js 22 and `npm ci`, validates frontend types and the Rust workspace before packaging, and runs Linux tests before packaging. The unnecessary macOS Homebrew GTK installation is removed; GTK is a Linux backend dependency. Both Linux runners and the Windows/macOS jobs are retained.

## Recovery verification

- `cargo check --workspace --locked`: passed locally.
- `npm ci`, `npm run check`, and `npm run build`: passed locally; Svelte reports zero errors and warnings.
- `cargo test --workspace --locked`: passed locally (67 tests). The initial sandbox run could not bind local mock HTTP ports; the rerun with loopback access passed.
- `npm run tauri build -- --debug --no-bundle -- --locked`: passed locally and produced the desktop executable.
- [CI run 35850040240](https://github.com/redog/WackyTheorem/actions/runs/35850040240) completed successfully for exact commit `46943f58fa982c79d44c5ea0000c51fba1ecd778`: Ubuntu, Fedora, Windows, and macOS all passed. Frontend/Rust checks and desktop packaging passed on all four runners; workspace tests passed on both Linux runners.
- Workflow YAML and diff checks passed. This result certifies the repair commit; subsequent commits still require their own applicable checks.
- `npm ci` reports five pre-existing dependency audit findings; dependency remediation is separate from this build repair.

## Supported temporal slice and verification

- Encrypted additive baseline and item snapshots retain source identities, payloads, corrections, tombstones, and event/recording times (D17).
- Queries reconstruct a committed boundary before filtering, with deterministic ordering and explicit truncation. Identical replay does not create a revision.
- Saved query definitions are revisioned; live views reevaluate and pinned views retain their boundary. Removing a view leaves evidence intact.
- Six history tests cover late evidence, tied timestamps, corrections, deletion/revival, atomic rollback, upgrade/reopen, replay, saved-query revision, overlapping views, and retention.
- File and mock Calendar pipeline tests also assert historical source payload preservation. The OAuth integration tests now serialize their shared token-store access after the full suite exposed a cross-test collision; production authentication behavior is unchanged.
- `cargo check --workspace --locked` and all 73 workspace tests pass locally. `npm ci`, frontend checking (zero errors/warnings), and the production frontend build pass. [CI run 35941840319](https://github.com/redog/WackyTheorem/actions/runs/35941840319) passed for exact commit `fc2b8bd8356b8dc78eff8b2372b299b4a10ef9ca`: Ubuntu, Fedora, Windows, and macOS. Both Linux workspace test jobs passed; all four desktop builds passed. This evidence applies to that implementation commit; this subsequent documentation record does not change runtime code.

### Desktop acceptance check

The Linux WebKitWebDriver walkthrough in `desktop/wkyt/tests/desktop_acceptance.py` passes on the repair branch (2026-09-24). It exercised the built desktop interface, a separate passphrase vault, the file pipeline, and loopback mock Calendar endpoints. Verified: recovery ceremony, cross-source query, overlapping live/pinned views, automatic live refresh after correction, unchanged pinned raw payload, app restart and passphrase unlock, persisted definitions, and removing a view without deleting either source. A screenshot of the synthetic results was inspected. No real Google account or personal vault was used; real Google OAuth and Windows/macOS interaction remain unverified by this walkthrough.

The first run against `13150c3` failed before the recovery ceremony: each desktop command created a new key store and discarded the entered passphrase. D18 retains one lazy key service per app state; the same walkthrough then passed. Eric reviewed and merged this repair in PR #35 on 2026-09-24. The baseline documentation commit `13150c3` passed four-platform CI run `35942541532`; that build success did not detect this runtime defect.

The current desktop debug build, locked frontend install, frontend checks (zero errors/warnings), and frontend build pass. `cargo check --workspace --locked` and all 73 workspace tests also pass. PR CI run `35998672034` passed for `5a67e55`, and post-merge CI run `35999177257` passed for `ab1042f`, on all four platforms. The acceptance test adds no runtime dependencies. The Ubuntu CI job now runs it under Xvfb against the release binary before artifact upload, with a ten-minute timeout and normal failure propagation. Local virtual-display acceptance passed against the debug binary; the exact pushed workflow must additionally pass with the Ubuntu release build. See its adjacent README for the exact invocation and boundaries.

### CI gate validation (2026-09-24)

- Workflow YAML parsed; test/build/acceptance/upload ordering, timeout, failure propagation, and retention of all four platform builds checked.
- `cargo check --workspace --locked`, all 73 workspace tests, `npm ci`, frontend checks (zero errors/warnings), and frontend production build passed locally.
- The full desktop walkthrough passed with X11, software rendering, and Xvfb using the existing debug build of the merged application. This change does not modify runtime code or the walkthrough.
- Ubuntu CI is the verification point for distribution packages and the release binary. Inspect that exact commit's Actions result before declaring the gate verified or starting evidence-consistency work.

The first resumed Ubuntu run failed during package installation because Ubuntu 26.04 replaced `webkit2gtk-driver` with `webkitgtk-webdriver`. The correction at `75eba3a` passed all four jobs in run `36048436691`, including the required release-binary desktop walkthrough. No check was skipped or weakened.

### Connector deletion verification

- File deletion and Calendar cancellation now retire the source and its two generated records in the existing atomic batch. Independent sources remain live; no record is hard-deleted.
- The file pipeline test checks current claim/evidence queries, a shared deletion boundary, earlier source payloads, quiet replay, and restoration without duplicate identities. The Calendar regression checks the same lifecycle, including reopening the encrypted vault and historical evidence-link targets.
- `cargo check --workspace --locked` and all 74 workspace tests pass locally. Locked frontend installation, frontend checks (zero errors/warnings), frontend build, and the desktop debug build pass.
- The desktop acceptance test now clears the earlier text filter and checks all three file record IDs, so it detects orphaned derivations rather than allowing them to be hidden by a query. The extended walkthrough passed locally under Xvfb, including deletion and restoration. Inspect the exact pushed commit's CI result before advancing.

### Next bounded work

Complete and verify connector deletion propagation first. Then review pinned claim/evidence provenance, followed by durable local capability outcomes; do not declare all of Roadmap A finished from item snapshot tests alone.

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

The preceding architecture-only commit reconciled nine control documents and checked local links/diffs. Build recovery is now complete with exact-commit CI evidence. The updated coding instructions require that evidence before future recovery claims and prohibit advancing through failing checks. Roadmap A is partially implemented; passing builds do not certify the unmet architecture guarantees in the inventory above.
