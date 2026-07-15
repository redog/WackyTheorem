# WackyTheorem: Promptware

[![Tauri CI](https://github.com/redog/WackyTheorem/actions/workflows/ci.yml/badge.svg)](https://github.com/redog/WackyTheorem/actions/workflows/ci.yml)

WackyTheorem is an experimental **cognitive operating environment**: a local-first attempt to reorganize personal computing around knowledge, provenance, intent, capabilities, specialized agents, negotiated trust, and human context instead of treating applications and files as the primary abstractions.

The current desktop application is deliberately modest. It ingests authorized personal data into an encrypted local vault and proves the memory substrate needed for the larger system. It is the seed, not the final product.

Read [`VISION.md`](VISION.md) for the north star, [`Spec.md`](Spec.md) for invariants and architecture, [`Roadmap.md`](Roadmap.md) for the evolutionary plan, and [`DECISIONS.md`](DECISIONS.md) for durable choices.

## What exists today

- **Encrypted vault:** SQLite + SQLCipher compiled from source, KEK/DEK hierarchy anchored to the OS keychain, recovery-key ceremony, and crash-safe key rotation.
- **Replay-safe ingestion:** bounded streaming connectors, opaque cursors, deterministic identity, tombstones, transactional batch-plus-cursor commits, and ack-after-commit behavior.
- **Connectors:** local `.json`/`.ics` import and Google Calendar ingestion.
- **Minimal interface:** a Svelte/Tauri viewer over the encrypted vault.
- **Cross-platform checks:** Rust tests and desktop build checks in CI.

Nothing sensitive should land on disk in plaintext, including database temporary storage.

## Where it is going

The next step is not “add more apps.” It is to transform imported records into a provenance-bearing LifeGraph of entities, events, claims, relationships, and evidence.

Later phases introduce:

- task-oriented capabilities instead of application ownership;
- temporary interfaces assembled for the current intent;
- many narrow agents rather than one omniscient assistant;
- visible uncertainty, disagreement, and evidence;
- contextual, revocable trust for external actions;
- optional, correctable models of human attention and cognitive state.

Files, shells, and existing applications remain available as compatibility and inspection surfaces. They stop being the only way the system understands the user's world.

## Repository layout

```text
crates/
  wkyt-core            domain types and connector contracts
  wkyt-vault           encrypted vault and key lifecycle
  wkyt-broker          bounded in-process transport
  wkyt-connector-file  local import connector
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

The test suite covers the vault lifecycle, recovery and rotation, tamper handling, crash replay, deterministic ingestion, and end-to-end connector-to-vault behavior.

## Development model

The repository is intended for mostly autonomous iteration guided by its Markdown control plane. Coding agents should read the documents in this order:

1. `VISION.md`
2. `Spec.md`
3. `Roadmap.md`
4. `DECISIONS.md`
5. `agents.md`
6. `IMPLEMENTATION_PLAN.md`

Eric steers the project by editing these documents, reviewing load-bearing decisions, and redirecting the loop when implementation drifts from the intended computing model.
