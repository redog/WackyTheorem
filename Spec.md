# Specification: WackyTheorem

> An unconventional cognitive operating environment, conceived through human–LLM iteration, built with a complete lack of seriousness and a full complement of engineering rigour.

## 1. Product definition

WackyTheorem is a local-first cognitive operating environment. It organizes personal computing around knowledge, provenance, intent, capabilities, agents, negotiated trust, and human context rather than treating applications and files as the primary user abstractions.

The long-term vision is defined in [`VISION.md`](VISION.md). This specification translates that vision into architectural guarantees and an executable development direction.

The current Tauri desktop application is the first substrate: an encrypted local memory system with reliable ingestion. It is not the final interaction model.

## 2. System invariants

These are load-bearing requirements. Implementation choices may change; these guarantees should not change without an explicit architectural decision and Eric's review.

1. **Local authority:** The user controls the canonical personal data store. Cloud services may be sources or optional compute providers, but they are not the source of authority.
2. **No unintended plaintext at rest:** Sensitive personal data, tokens, keys, intermediate data, indexes, temporary files, logs, crash dumps, and derived artifacts must not be written to disk in plaintext.
3. **Provenance before synthesis:** Imported and generated knowledge retains source identity, timestamps, transformation history, and uncertainty.
4. **Files are compatibility artifacts:** Files remain importable, exportable, and inspectable, but the canonical model is an evolving graph of entities, events, claims, relationships, and evidence.
5. **Capabilities over applications:** New functionality should be expressed as composable capabilities with explicit inputs, outputs, authority, and side effects—not as isolated application silos.
6. **Plural intelligence:** Intelligence is composed from narrow agents and deterministic tools. No single model receives implicit universal authority.
7. **Inspectable execution:** Plans, evidence, actions, permissions, agent contributions, and resulting artifacts must remain visible and auditable.
8. **Negotiated trust:** Access is contextual, least-privileged, revocable, and tied to a purpose and retention policy.
9. **Human agency:** Models of human context are uncertain, correctable, optional, and used to cooperate rather than manipulate.
10. **Graceful degradation:** Core memory and retrieval must remain useful without an LLM, network access, or a particular vendor.

## 3. Conceptual architecture

### 3.1 Knowledge substrate

The canonical data model is the **LifeGraph**: a temporal, provenance-preserving graph containing at least:

- entities such as people, organizations, places, devices, accounts, and projects;
- events and states with temporal bounds;
- claims with epistemic type and confidence;
- relationships between entities, events, claims, and evidence;
- source artifacts and unaltered raw payloads;
- transformations, revisions, and tombstones;
- human and agent-authored decisions.

The current relational SQLite schema may represent this graph incrementally. A dedicated graph database is not required.

### 3.2 Ingestion

Connectors translate authorized external sources into durable, replayable changes.

Required properties:

- bounded streaming and backpressure;
- opaque connector-defined sync positions;
- crash-safe resume from the last committed position;
- deterministic stable identity;
- idempotent application of repeated batches;
- tombstones or equivalent deletion semantics;
- preservation of original payloads and source metadata;
- explicit error classification and authorization state.

Current reference implementation: Rust async connectors, bounded in-process transport, transactional batch-plus-cursor commits, deterministic UUIDv5 identities, and SQLCipher-backed SQLite.

### 3.3 Capabilities

A capability is a narrow, composable operation. It declares:

- purpose and semantic contract;
- typed inputs and outputs;
- required data access;
- possible side effects;
- retention behavior;
- expected evidence and audit output;
- failure and rollback behavior.

Capabilities may wrap local code, command-line tools, external APIs, existing applications, or agent reasoning. Existing applications are acceptable engines behind a capability boundary.

### 3.4 Agents

A runtime agent is a specialized reasoning participant with:

- a narrow role;
- bounded authority;
- declared capabilities;
- explicit context access;
- an output contract;
- provenance and uncertainty requirements;
- lifecycle and resource limits.

The system must support disagreement and review. Planner, implementer, verifier, skeptic, and domain specialist are distinct roles even when one model temporarily fills several of them.

### 3.5 Human context

The system may represent the human as a first-class participant with goals, active tasks, expertise, preferences, interruptions, and uncertain cognitive state.

Inferred state such as fatigue, confidence, interruptibility, or working-memory load must never be asserted as fact. Each estimate requires provenance, confidence, expiry, user visibility, correction, and disable controls.

### 3.6 Trust and action

Read and write authority is granted to capabilities and agents for a defined purpose. Actions with external side effects require an inspectable plan and an authorization policy appropriate to their risk.

The long-term model is contextual capability leases rather than permanent application permissions.

### 3.7 Intelligence and retrieval

LLMs are optional reasoning engines over retrieved graph context. Retrieval must preserve provenance and distinguish source facts from generated synthesis.

Answers should expose supporting evidence, conflicting evidence, temporal scope, and uncertainty. Deterministic queries and tools should be preferred where they can answer reliably.

### 3.8 Presentation

The interface is task-oriented and may be assembled dynamically from reusable views and capabilities. A temporary interface should be able to exist only for the duration of a task.

The shell, source files, and conventional applications remain available as inspectable compatibility surfaces.

## 4. Reference implementation constraints

These constraints govern the current implementation unless superseded in `DECISIONS.md`.

- **Desktop shell:** Tauri 2.x with a Rust backend and Svelte 5 frontend.
- **Language:** Stable Rust, current repository edition.
- **Vault:** SQLite through `rusqlite` with bundled SQLCipher and encrypted temporary storage.
- **Keys:** Random DEK, wrapped through an OS-keychain-anchored KEK, with recovery and rotation support.
- **Connectors:** Native Rust initially; sandboxed connector execution remains an architectural objective.
- **CI:** GitHub Actions. Linux is required; macOS and Windows support may continue where practical.
- **Dependency discipline:** Every dependency requires an explicit justification.

## 5. Current completed substrate

The repository already contains:

- the encrypted vault and recovery lifecycle;
- a replay-safe connector contract;
- bounded ingestion with transactional cursor commits;
- deterministic item identity and tombstones;
- file and Google Calendar ingestion;
- a minimal viewer;
- cross-platform CI coverage.

This completes the initial memory substrate. New work should now begin moving from **records in a vault** toward **provenance-bearing knowledge and capability composition**.

## 6. Current development objective

The next objective is a thin vertical slice proving the future computing model:

1. Convert ingested records into explicit entities, events, claims, relationships, and evidence.
2. Preserve provenance from source record through every derived assertion.
3. Accept a user intent that crosses more than one source.
4. Compose at least two narrow capabilities or agents to answer it.
5. Present the answer with evidence and uncertainty in a task-specific interface.
6. Perform no external side effect without explicit authorization.

A suitable milestone is:

> Ask “What changed about Project Alpha last week, and what evidence supports that?” and receive a temporal synthesis assembled from multiple sources, with source links, claim confidence, and visible agent/tool contributions.

## 7. Development behavior under ambiguity

The project is deliberately exploratory. Agents should make small, reversible, opinionated decisions when the specification is silent.

They must not stall merely because the final architecture is unknown. They should:

- preserve the system invariants;
- prefer interfaces that expose future capability and provenance boundaries;
- document durable choices in `DECISIONS.md`;
- record uncertain experiments in `IMPLEMENTATION_PLAN.md`;
- escalate only decisions that materially affect security, privacy, irreversible data formats, external side effects, or the north-star model.

## 8. Operator constraints

- Nothing sensitive in plaintext on disk. This is non-negotiable.
- Auth, encryption, trust, and externally acting capabilities require Eric's review before merge.
- No unnecessary dependencies.
- Linux must remain a first-class development platform.
- Prefer recoverable filesystem operations (`trash`) over destructive deletion.
- Do not preserve a weak abstraction merely because it already exists. Migrate deliberately when a stronger model is proven.

## 9. Repository identity

Repository: `https://github.com/redog/WackyTheorem`

Commits authored by `imp <imp@automationwise.com>`. Eric's GitHub account (`redog`) is the repository owner and GitHub identity.
