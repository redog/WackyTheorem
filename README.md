# WackyTheorem: Promptware

[![Tauri CI](https://github.com/redog/WackyTheorem/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/redog/WackyTheorem/actions/workflows/ci.yml)

WackyTheorem is an experimental **cognitive operating environment**: a local-first attempt to reorganize personal computing around knowledge, temporal history, projected views, intent, capabilities, provenance, and negotiated trust instead of treating applications and files as the primary abstractions.

The desktop is the bootstrap environment. The repository contains encrypted ingestion and semantic/capability prototypes; their observed scope and known gaps are recorded in [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md).

Read [VISION.md](VISION.md) for why, [Spec.md](Spec.md) for stable invariants and architecture, [Roadmap.md](Roadmap.md) for the major capability sequence, and [DECISIONS.md](DECISIONS.md) for durable choices. The implementation plan alone owns current status and the active milestone.

## Computing model

- **LifeGraph:** meaning, relationships, claims, and evidence.
- **Temporal stream:** reconstructable knowledge and activity history.
- **Substreams:** overlapping saved/live queries and views, without owning source records.
- **Capabilities:** bounded operations under negotiated authority.
- **Agents:** optional reasoning participants, with deterministic tools preferred where reliable.

Files and original source records remain first-class evidence. Existing applications remain useful editors and engines. The intended loop connects capture, history, derivation, queries, provenance-bearing summaries, and authorized watchers/reminders/actions. These are architectural concepts, not a claim that the full loop is implemented.

Sensitive personal data and intermediate storage must not land on disk in plaintext.

## Repository layout

```text
crates/
  wkyt-core            domain types and connector contracts
  wkyt-vault           encrypted vault and key lifecycle
  wkyt-broker          bounded in-process transport
  wkyt-connector-file  local import connector
  wkyt-connector-google Google Calendar ingestion
  wkyt-host            ingestion orchestration
desktop/wkyt           Tauri backend and Svelte frontend
```

## Build and run

Prerequisites: Node.js, Rust, and the Tauri 2 prerequisites for your platform.

```bash
git clone https://github.com/redog/WackyTheorem.git
cd WackyTheorem/desktop/wkyt
npm install
npm run tauri dev
```

## Test

```bash
cargo test --workspace
cd desktop/wkyt && npm run check
```

The test sources cover vault lifecycle, recovery and rotation, tamper handling, crash replay, deterministic ingestion, and connector-to-vault behavior. See the implementation plan for actual verification results and known baseline gaps.

## Development model

The repository is intended for mostly autonomous iteration guided by its Markdown control plane. Coding agents should read the documents in this order:

1. `VISION.md`
2. `Spec.md`
3. `Roadmap.md`
4. `DECISIONS.md`
5. `agents.md`
6. `IMPLEMENTATION_PLAN.md`

Before substantial work, reconcile the plan and roadmap against code and repair inconsistent status claims. `agents.md` governs coding practice; it does not prescribe a runtime team.

Eric steers the project by editing these documents, reviewing load-bearing decisions, and redirecting the loop when implementation drifts from the intended computing model.
