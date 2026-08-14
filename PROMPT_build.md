0a. Read `VISION.md`, `Spec.md`, `Roadmap.md`, `DECISIONS.md`, and `agents.md`. Read `IMPLEMENTATION_PLAN.md` if present. Inspect `crates/` and `desktop/wkyt/`; do not assume roadmap work is missing until you search for it.

You are not merely extending an encrypted CRUD desktop application. You are building the earliest substrate of a cognitive operating environment whose primary abstractions are knowledge, provenance, intent, capabilities, agents, negotiated trust, and human context. The current vault and connectors are foundations, not the destination.

Choose the smallest high-value vertical slice that advances the current roadmap milestone. Prefer a demonstrable path from ingestion → knowledge/provenance → query or capability → inspectable interface over isolated framework construction.

Use parallel subagents for bounded searches and independent review. Give each subagent a narrow role and require it to return evidence, uncertainty, and concrete recommendations. Do not delegate final architectural authority to a panel vote.

Before changing code:

1. identify the relevant system invariant and roadmap outcome;
2. search for existing implementation and tests;
3. record the intended slice in `IMPLEMENTATION_PLAN.md`;
4. identify provenance, trust, plaintext-at-rest, and migration risks.

While implementing:

- preserve raw source payloads and source identity;
- retain provenance through every derived entity, event, claim, relationship, summary, or action;
- prefer deterministic logic before introducing LLM inference;
- express reusable operations as narrow capability contracts rather than app-specific handlers;
- make uncertainty and conflicting evidence representable;
- make changes reversible and avoid unnecessary compatibility layers;
- use `trash` rather than `rm` for filesystem work.

Do not stall for ordinary ambiguity. Make the smallest reversible decision, document durable choices in `DECISIONS.md`, and continue. Open `needs-human` only for load-bearing security, trust, irreversible schema, destructive migration, or external-action decisions. Auth, encryption, trust, and externally acting capabilities require Eric's review before merge.

Test the completed slice in the dev environment. Resolve regressions caused by the change. Update `IMPLEMENTATION_PLAN.md` with findings and remove completed tactical items. Put only reusable operational commands in `agents.md`.

### Mandatory Build and Verification Checks

You MUST NOT conclude a completion, mark a milestone as finished, or commit code if the build or tests fail. You are strictly required to verify the build locally before concluding your work.

If you encounter missing dependencies in your sandbox environment (like missing `gobject-2.0` or `glib-2.0`), you must install them first based on the CI requirements. For example, on Ubuntu you may need:
`sudo apt-get update && sudo apt-get install -y build-essential curl libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libglib2.0-dev libsqlite3-dev libsoup-3.0-dev pkg-config cmake libssl-dev libx11-dev libxkbcommon-dev libsecret-1-dev libdbus-1-dev libgdk-pixbuf-xlib-2.0-dev xdg-utils`

Before you mark a task as completed, you MUST successfully run and verify the exit codes of the following commands:
1. `cargo check --workspace` (from the repository root)
2. `cargo test --workspace` (from the repository root)
3. `cd desktop/wkyt && npm install`
4. `cd desktop/wkyt && npm run check`

If any of these commands fail, you MUST fix the underlying issue before proceeding. Do not iterate over instructions or milestones blindly while ignoring these failures.

When the increment is complete and all mandatory checks pass: update relevant documentation, commit as `imp <imp@automationwise.com>`, push, and create the next patch tag. Do not create a tag merely because the repository happens to build before meaningful roadmap progress was made.
