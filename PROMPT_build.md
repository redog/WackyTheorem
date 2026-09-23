0a. Read `VISION.md`, `Spec.md`, `Roadmap.md`, `DECISIONS.md`, and `agents.md`. Read `IMPLEMENTATION_PLAN.md`. Inspect `crates/` and `desktop/wkyt/`; do not assume roadmap work is missing until you search for it.

Build toward the model in `Spec.md`: LifeGraph supplies meaning, the temporal stream supplies history, substreams supply organization, capabilities supply action, and agents are optional reasoning participants. Preserve local authority, encryption, provenance, negotiated trust, and deterministic-before-LLM behavior.

Reconcile the active milestone in `IMPLEMENTATION_PLAN.md` against the code and the major sequence in `Roadmap.md`. Repair inconsistent control documents before substantial implementation. A checked box or historical phase label is not evidence of working behavior.

If the baseline fails, repairing it is the active slice. Do not start roadmap features or reinterpret failing checks as completion. Separate compiler/test failures from runner or dependency failures and record their evidence in the plan.

Choose the smallest useful slice of capture → history → knowledge → query/substream → summary → watch/remind/act → resulting history. Keep implementation scope bounded; architectural concepts are not instructions to build every subsystem at once.

Before changing code:

1. identify the relevant system invariant and roadmap outcome;
2. search for existing implementation and tests;
3. record the intended slice in `IMPLEMENTATION_PLAN.md`;
4. identify provenance, trust, plaintext-at-rest, and migration risks.

While implementing:

- preserve raw source payloads and source identity as first-class evidence;
- keep organization in query/view projections rather than owning containers;
- preserve event/recording-time distinctions and durable history across changes, decisions, permissions, and action outcomes;
- retain provenance through every derived entity, event, claim, relationship, summary, or action;
- prefer deterministic logic before introducing LLM inference;
- express reusable operations as narrow capability contracts rather than app-specific handlers;
- make uncertainty and conflicting evidence representable;
- make changes reversible and avoid unnecessary compatibility layers;
- use `trash` rather than `rm` for filesystem work.

Do not stall for ordinary ambiguity. Make the smallest reversible decision, document durable choices in `DECISIONS.md`, and continue. Open `needs-human` only for load-bearing security, trust, irreversible schema, destructive migration, or external-action decisions. Auth, encryption, trust, and externally acting capabilities require Eric's review before merge.

Test the completed slice in the dev environment. Resolve regressions caused by the change. Update `IMPLEMENTATION_PLAN.md` with findings and remove completed tactical items. Put only reusable operational commands in `agents.md`.

### Documentation-only verification

For changes limited to prose documentation, inspect the complete diff, validate local links and references, and reconcile status claims with code. Record any baseline failures discovered. Such a change may be committed and pushed without installing or rebuilding the application; it must not claim a runtime milestone or a green build. Do not create a release tag for a documentation-only reconciliation.

### Mandatory Build and Verification Checks for Code Changes

You MUST NOT conclude a completion, mark a milestone as finished, or commit code if the build or tests fail. You are strictly required to verify the build locally before concluding your work.

If you encounter missing dependencies in your sandbox environment (like missing `gobject-2.0` or `glib-2.0`), you must install them first based on the CI requirements. For example, on Ubuntu you may need:
`sudo apt-get update && sudo apt-get install -y build-essential curl libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libglib2.0-dev libsqlite3-dev libsoup-3.0-dev pkg-config cmake libssl-dev libx11-dev libxkbcommon-dev libsecret-1-dev libdbus-1-dev libgdk-pixbuf-xlib-2.0-dev xdg-utils`

Before you mark a code task as completed, you MUST successfully run and verify the exit codes of the following commands:
1. `cargo check --workspace --locked` (from the repository root)
2. `cargo test --workspace --locked` (from the repository root)
3. `cd desktop/wkyt && npm ci` (Node.js 22, matching CI)
4. `cd desktop/wkyt && npm run check`
5. `cd desktop/wkyt && npm run build`

If any of these commands fail, you MUST fix the underlying issue before proceeding. Do not iterate over instructions or milestones blindly while ignoring these failures.

When local checks pass, update documentation with their actual results, commit as `imp <imp@automationwise.com>`, and push. Inspect CI for that exact commit and resolve failures before declaring the build baseline restored or advancing to another milestone. Record a queued, unavailable, or failing runner explicitly; never treat it as a pass. Do not disable tests, weaken assertions, remove a required platform, or add `continue-on-error` to obtain a green result.

A trigger-only CI diagnostic commit may be pushed when explicitly requested even if the known baseline is failing; label it as diagnostic rather than completed implementation. Do not create release tags for baseline repairs or documentation reconciliation. Tag only an intentional release after relevant checks pass; incrementing a patch tag is not evidence of progress.
