# Agent Guidelines

## Principles
- Roadmap.md and DECISIONS.md are source of truth. Never contradict them without opening an issue first.
- Prefer small focused PRs over large ones.
- If uncertain about architecture, open an issue and stop. Do not guess.

## Merge Criteria
- All CI checks must pass
- No new dependencies without a comment explaining why
- Auth, encryption, and secrets handling requires human review — label PR accordingly

## Roles (any agent may fill any role per PR)
- **Author**: implements, writes PR description referencing the relevant roadmap phase
- **Reviewer**: checks for spec drift, security issues, test coverage
- **Tester**: writes or updates tests, verifies CI passes

## What to do when stuck
Open a GitHub issue tagged `needs-human`. Do not proceed.
