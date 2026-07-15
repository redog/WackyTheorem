# Agent Guidelines

This file governs coding agents working on the repository. Runtime agents inside WackyTheorem are specified separately in `Spec.md`.

## Sources of truth

Read in this order:

1. `VISION.md` — north star and computing model.
2. `Spec.md` — invariants and current architectural direction.
3. `Roadmap.md` — current phase and milestone.
4. `DECISIONS.md` — durable implementation and architecture decisions.
5. `IMPLEMENTATION_PLAN.md` — current tactical state, findings, and next work.

When documents conflict, preserve the invariants in `Spec.md`, record the conflict, and make the smallest reversible choice that advances the current roadmap milestone.

## Working principles

- Prefer thin end-to-end slices over isolated infrastructure.
- Search the codebase before assuming work is missing.
- Preserve provenance across every transformation.
- Treat files and applications as compatibility surfaces, not default domain boundaries.
- Express reusable functionality as narrow capabilities with explicit contracts.
- Prefer deterministic tools over LLM inference where they can answer reliably.
- Keep agent authority narrow and make uncertainty visible.
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

- Relevant tests pass; CI remains green.
- No sensitive plaintext is written to disk, logs, fixtures, or crash artifacts.
- New dependencies include a justification.
- New transformations retain provenance.
- New capabilities declare inputs, outputs, authority, side effects, and failure behavior.
- Auth, encryption, trust, and external-action changes receive human review.
- Documentation reflects why the increment matters to the current roadmap milestone.

## Operational notes

Keep this section brief and limited to commands or environment facts needed by future coding agents. Progress and status belong in `IMPLEMENTATION_PLAN.md`.
