# Agent Guidelines

This file governs coding agents working on the repository. Runtime agents inside WackyTheorem are specified separately in `Spec.md`.

## Sources of truth

Read in this order:

1. `VISION.md` — north star and computing model.
2. `Spec.md` — stable invariants and architecture.
3. `Roadmap.md` — major capability sequence.
4. `DECISIONS.md` — durable implementation and architecture decisions.
5. `IMPLEMENTATION_PLAN.md` — current tactical state, findings, and next work.

`IMPLEMENTATION_PLAN.md` alone owns the active milestone and current status. Before choosing work, reconcile its declaration and the roadmap outcome against implemented code and tests. Do not begin a milestone merely because an old spec section or unchecked roadmap item says it is pending. Repair control-document inconsistencies before substantial implementation, preserving `Spec.md` invariants and distinguishing source-code presence from verified behavior. Record concrete evidence and gaps in the plan; record durable architectural changes and supersession in `DECISIONS.md`.

This file governs coding practice, not a required runtime role hierarchy. Use the coding environment's ordinary planning and review tools as useful; no fixed team or mandatory role multiplication is prescribed.

## Working principles

- Prefer thin end-to-end slices over isolated infrastructure.
- Search the codebase before assuming work is missing.
- Preserve provenance across every transformation.
- Treat files and original source records as first-class evidence; applications may remain editors and capability engines.
- Organize through queries and views over canonical records; a workspace must not own or relocate its evidence.
- Preserve reconstructable temporal history for durable knowledge, decisions, authorizations, operations, and outcomes; distinguish event time from recording time.
- Keep summaries tied to bounded, identifiable source revisions, and route watcher actions through capability authority.
- Express reusable functionality as narrow capabilities with explicit contracts.
- Prefer deterministic tools over LLM inference where they can answer reliably.
- Keep capability and optional reasoning authority narrow and make uncertainty visible.
- Avoid placeholders and permanent abstractions created only to satisfy a demo.
- Prefer small, focused commits and recoverable filesystem operations.

## Autonomy and escalation

Do not stop for ordinary ambiguity. Make and document a reversible decision.

Escalate with `needs-human` only when a choice materially affects:

- encryption, authentication, or secret handling;
- plaintext-at-rest guarantees;
- irreversible vault or public schema compatibility;
- external side effects or trust policy;
- destructive data migration;
- the north-star abstractions in `VISION.md`.

For escalated questions, continue unrelated safe work when possible.

## Merge criteria

- Establish a passing baseline before feature work. Relevant local checks must pass, and CI must be inspected for the exact pushed commit before declaring recovery complete. Report runner failures or queued jobs separately from code failures. Follow `PROMPT_build.md` for commands and documentation-only validation.
- No sensitive plaintext is written to disk, logs, fixtures, or crash artifacts.
- New dependencies include a justification.
- New transformations retain provenance.
- New capabilities declare inputs, outputs, authority, side effects, and failure behavior.
- Auth, encryption, trust, and external-action changes receive human review.
- Documentation reflects the active slice in `IMPLEMENTATION_PLAN.md` and its roadmap outcome, with no duplicate current-status claims in `Spec.md` or `Roadmap.md`.

## Operational notes

Use Node.js 22 and `npm ci` for the locked frontend dependencies. Run Rust checks with `--locked`. CI checks frontend types and the Rust workspace before desktop packaging, and runs workspace tests on Linux. Progress and per-commit results belong in `IMPLEMENTATION_PLAN.md`.
