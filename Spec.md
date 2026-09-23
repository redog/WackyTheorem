# Specification: WackyTheorem

## 1. Purpose and document boundaries

WackyTheorem is a local-first cognitive operating environment connecting knowledge, temporal history, intent, provenance, capabilities, and negotiated trust.

[VISION.md](VISION.md) defines why the project exists and its north star. This specification defines stable invariants and architecture, not implementation status. [Roadmap.md](Roadmap.md) orders major capabilities; [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md) owns current tactical state, evidence, gaps, and the active milestone. [agents.md](agents.md) governs coding work. [DECISIONS.md](DECISIONS.md) records durable choices and supersession; [TOOS.md](TOOS.md) holds non-controlling exploration.

Requirements below describe the target architecture. Their presence does not certify that the current code fulfills them.

## 2. System invariants

These are load-bearing requirements. Changes require an explicit architectural decision and Eric's review.

1. **Local authority:** The user controls the canonical personal data store. Cloud services may supply data or optional computation; they do not become the source of authority.
2. **No unintended plaintext at rest:** Sensitive personal data, tokens, keys, intermediate data, indexes, temporary files, logs, crash dumps, and derived artifacts must not be written to disk in plaintext. A capability grant does not waive this requirement.
3. **Provenance before synthesis:** Imported and generated knowledge retains source identity, timestamps, transformation history, and uncertainty. Derived artifacts do not replace their evidence.
4. **Files are source artifacts:** Files and other original source records are first-class evidence, preserved with their available raw content and source metadata. They remain inspectable and exportable without becoming organizational containers.
5. **Organization is projected:** Projects, workspaces, collections, dashboards, and task views are queries or projections over canonical knowledge and history. Views may overlap; creating, changing, or deleting a view must not move, duplicate, own, or delete its underlying records. Explicit membership assertions may be knowledge, but a presentation container is not a data-ownership boundary.
6. **Temporal continuity:** Every durable observation, change, derivation, decision, authorization, capability invocation, action outcome, optional agent result, correction, and tombstone participates in reconstructable temporal history. The system must be able to explain what was known, believed, proposed, authorized, attempted, or changed at a given time without relying on an application's private history.
7. **Capabilities over applications:** Reusable operations have explicit inputs, outputs, authority, retention, side effects, and failure behavior. Applications may implement capabilities without owning the user's information model.
8. **Deterministic before LLM:** Prefer deterministic tools and queries where reliable. Agents and models are optional reasoning participants; neither a fixed role hierarchy nor multiple agents is required. No model gains implicit universal authority.
9. **Inspectable execution and negotiated trust:** Plans, evidence, permissions, operations, contributions, and outcomes remain auditable. Authority is contextual, least-privileged, revocable, and tied to purpose and retention.
10. **Human agency and graceful degradation:** Core memory and retrieval remain useful without an LLM, network access, or a particular vendor. Inferred human state, if introduced, is optional, uncertain, visible, correctable, and disableable; it must not be used to manipulate.

## 3. Conceptual architecture

### 3.1 LifeGraph: meaning and evidence

LifeGraph is the semantic model: entities (including people and projects), events and states, claims, relationships, source artifacts, and decisions. Claims distinguish observation, imported assertion, inference, hypothesis, and generated suggestion; disagreement and conflicting evidence remain representable. Entity resolution must preserve ambiguity instead of silently merging identities.

Sources retain their identities and available unaltered payloads. Transformations link outputs to the specific inputs and revisions they used, their method, responsible actor or capability, and uncertainty. A file's external path is source metadata, not the sole identity or provenance of everything derived from it.

### 3.2 Temporal stream: reconstructable history

The temporal stream records how knowledge and system activity evolve alongside LifeGraph. A history entry needs stable identity, affected records or revisions, event and recording context, provenance, and links to the activity that caused it. Corrections and tombstones preserve the distinction between prior knowledge and current state.

Distinguish **when something occurred or applied** from **when the system learned or recorded it**. Late-arriving evidence must not rewrite what the system previously knew. Equal timestamps need a stable ordering rule; wall-clock order alone must not imply causality. Queries must declare their time axis and distinguish an as-known-at view from an effective-time view.

Recording a proposal, granting authorization, attempting an action, and observing its outcome are distinct events. History must not imply success from intent or approval. External failures and uncertain outcomes remain visible. Replay must preserve identity and avoid duplicate effects.

History reconstruction is bounded by authorized retention and deletion policy. Expired or deliberately removed evidence must be represented as unavailable rather than silently reconstructed or retained against policy. History inspection is not a promise that external effects can be undone.

These semantics do not mandate a global event bus, a dedicated graph database, a complete event-sourcing rewrite, or distributed consensus. Relational storage and incremental history support are acceptable when they preserve the invariants.

### 3.3 Substreams: saved and live queries

A substream is a query projection over LifeGraph and temporal history, with explicit scope, filters, time semantics, and ordering. It can be temporary or saved. A saved live substream reevaluates as relevant records or time change; a snapshot pins the input revisions and temporal boundary for reproducibility. Views can overlap and compose without copying source records.

A saved query is itself durable, revisioned knowledge. Its definition and parameters must remain inspectable. Cached or materialized results remain replaceable projections. Query access is constrained by current authority; saving a view does not grant new access.

A workspace presents one or more substreams and capability results. Closing it removes presentation state, not durable sources, decisions, or results. Manual grouping can be represented by explicit relationships queried by the view.

### 3.4 Future-time semantics

Reminders, commitments, expected events, and scheduled operations share the temporal model. The following concepts are distinct; records use only those relevant to their meaning:

| Concept | Meaning |
| --- | --- |
| `effective_at` | When an assertion, state, or change applies. |
| `due_at` | When a commitment is due; passing it does not establish completion. |
| `not_before` | Earliest permitted eligibility or execution time. |
| `expires_at` | When validity or authority ends; expiry does not itself prove deletion. |
| `expected_at` | Predicted occurrence time, with uncertainty rather than observed status. |
| `observed_at` | When the source or observer actually observed something. |

Recording/ingestion time remains separate from these concepts. A future item can be known now without being an observed event. Arrival of a deadline may make a view or watcher eligible; it neither proves the event occurred nor authorizes action. Preserve timezone and precision where relevant, and represent unknown times without inventing observations. Concrete schema, recurrence, and scheduling machinery should follow a demonstrated workflow.

### 3.5 Typed summaries

A summary is a provenance-bearing derived artifact over a bounded input set, not a replacement for its sources or an inherently LLM-generated response. Its type may be a table, chart, timeline, task list, or narrative.

A durable summary retains its query identity and revision (when query-derived), input identities and revisions or an equivalent reproducible input-set reference, temporal bounds, generation method/version, generation timestamp, uncertainty, and revision relationship to prior summaries. New evidence may make a summary stale; refreshing it creates an inspectable new derivation. Source access and retention constraints also apply to summaries.

### 3.6 Watchers and triggers

A watcher couples a saved substream with an explicit deterministic predicate or temporal condition. A match may produce a notification, request a capability invocation, or optionally request reasoning. Automation does not require an agent.

Watch definitions, evaluation scope, triggering evidence, and resulting operations must be inspectable. Define enable/disable behavior, replay/deduplication, missed-time handling, and feedback-loop bounds for each implemented workflow. Reevaluation must not repeatedly execute an external action by accident. Recheck authority, expiry, and relevant inputs at execution time; a saved query or elapsed deadline is not an authorization grant.

### 3.7 Ingestion

Connectors translate authorized sources into durable, replayable changes with:

- bounded streaming and backpressure;
- opaque connector-defined sync positions and crash-safe resume;
- stable identity and idempotent batch application;
- transactional change-plus-cursor commits;
- tombstones or equivalent deletion semantics;
- original payloads and source metadata;
- explicit error classification and authorization state.

Ingestion supplies source evidence and history; deterministic derivations add semantic relationships without erasing the originals.

### 3.8 Capabilities, trust, and optional reasoning

A capability declares purpose, typed inputs and outputs, required access, side effects, retention, expected audit evidence, and failure or rollback behavior. Capabilities may wrap local code, command-line tools, external APIs, existing applications, or optional reasoning.

Authority is a contextual grant bounded by purpose, data, operation, duration, and retention. External effects require an inspectable plan and a risk-appropriate authorization policy. Denial, revocation, expiry, attempts, and outcomes belong in temporal history. A permission prompt alone is not the complete lease or audit model.

An optional reasoning participant consumes explicitly bounded context and returns outputs with provenance and uncertainty through capability contracts. Its conclusions may be challenged by evidence or review without prescribing named runtime roles. Deterministic retrieval, summarization, and watchers stand on their own.

### 3.9 Presentation and human intent

Task-oriented interfaces project knowledge and history and expose supporting evidence, conflicts, temporal scope, and uncertainty. Explicit goals, tasks, and commitments are ordinary semantic and temporal records. Speculative attention scheduling and inferred cognitive-state models remain exploratory in `TOOS.md`.

## 4. Reference implementation constraints

Unless superseded by a durable decision:

- **Desktop:** Tauri 2.x, Rust backend, Svelte 5 frontend.
- **Language:** Stable Rust, repository edition.
- **Vault:** SQLite through `rusqlite` with bundled SQLCipher; sensitive temporary storage remains encrypted or in memory.
- **Keys:** Random DEK wrapped by an OS-keychain-anchored KEK, with recovery, rotation, and the documented headless-Linux fallback.
- **Connectors:** Native Rust initially; third-party isolation must precede granting untrusted code authority.
- **CI:** GitHub Actions; Linux is first-class, with macOS and Windows where practical.
- **Dependencies:** Each addition needs a justification.

These choices do not require separate stores for graph, stream, and views. Incremental implementation must state which architectural guarantees remain unmet in `IMPLEMENTATION_PLAN.md`.

## 5. Operator constraints

- Auth, encryption, trust, and externally acting capability changes require Eric's review before merge.
- No unnecessary dependencies or sensitive plaintext on disk.
- Prefer recoverable filesystem operations over destructive deletion.
- Migrate weak abstractions deliberately when a stronger model is proven.

Coding workflow, verification, and ambiguity handling belong in `agents.md` and `PROMPT_build.md`.

## 6. Repository identity and conceptual source

Repository: `https://github.com/redog/WackyTheorem`. Commits are authored by `imp <imp@automationwise.com>`; Eric's GitHub identity is `redog`.

Freeman and Gelernter's *Lifestreams: A Storage Model for Personal Data* (SIGMOD Record 25(1), March 1996, pp. 80–86) informs temporal continuity, dynamic substreams, summaries, and future information. WKYT adds its own provenance, epistemic state, encryption, local authority, and negotiated-trust requirements. The paper's UI, client/server design, and document operations are not implementation mandates. See D16 in `DECISIONS.md`.
