# WackyTheorem Roadmap

This document orders major capabilities toward the [vision](VISION.md), subject to [Spec.md](Spec.md). It does not independently declare a current milestone or build status. [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md) owns the active slice, code-backed inventory, verification, and remaining gaps.

The sequence below replaces the old phase numbering. Earlier commits and decisions retain their historical phase labels; those labels must not be interpreted as current work assignments or evidence that an entire capability is finished.

## Foundation: encrypted memory and semantic knowledge

Preserve the existing vault, key lifecycle, replay-safe ingestion, file and Calendar connectors, claim/evidence relationships, and basic inspection surfaces. Extend those foundations through useful slices rather than rebuilding them because an old checklist says they are pending.

**Capability boundary:** authorized source records become inspectable knowledge with stable identity, original evidence, uncertainty, and reliable recovery.

The code inventory and its limitations are maintained in the implementation plan. Source presence alone is not proof of a passing build or a complete architectural guarantee.

## A. Temporal continuity and projected organization

Connect LifeGraph semantics to reconstructable history and saved queries.

Outcomes:

- distinguish event/effective time from observation and recording time;
- preserve revisions, corrections, tombstones, and durable activity history;
- query across authorized sources and inspect earlier knowledge;
- save overlapping substreams with explicit scope, time axis, and ordering;
- keep views independent of source ownership and support live reevaluation.

**Demonstration:** “Show me Project Alpha” presents file and Calendar evidence plus derived claims; a saved filter updates with relevant changes, and the user can distinguish what was known earlier from what is known now. Removing the view leaves its evidence intact.

## B. Typed summaries and temporal follow-through

Build on bounded substreams to make the capture-to-action loop useful.

Outcomes:

- provenance-bearing summaries with pinned inputs, generation method, and revision relationships;
- appropriate summary forms such as tables, timelines, task lists, and narratives;
- explicit future-time semantics for commitments, reminders, expectations, and scheduled operations;
- deterministic watchers driven by changes or temporal conditions;
- inspectable trigger evidence, replay handling, and resulting history.

**Demonstration:** summarize a project substream, watch a meaningful change, and surface a due commitment. Each result links to its inputs and temporal scope. No LLM is needed for this loop.

## C. Bounded capabilities and negotiated action

Mature the existing capability and approval prototypes as workflows require them. Authorization and audit requirements apply from the first action in every stage; this stage is not permission to defer safety in A or B.

Outcomes:

- enforceable input, output, access, retention, and side-effect contracts;
- contextual grants with revocation and expiry;
- durable proposal, authorization, attempt, and outcome history;
- temporary interfaces over substreams and results;
- dry runs and compensating actions where appropriate;
- isolation before introducing untrusted connectors or capabilities.

**Demonstration:** a watcher proposes a bounded action, the applicable policy authorizes or denies it, and the user can inspect the actual outcome and evidence. Replays do not duplicate external effects.

## D. Optional reasoning and broader interoperability

Add inference only where a demonstrated workflow benefits from it. There is no runtime role-team milestone.

Outcomes:

- optional local models or other explicitly authorized compute over bounded retrieved context;
- provenance and uncertainty for generated conclusions;
- stronger retrieval or embeddings only when justified by evidence;
- new connectors, browser ingestion, or sandbox mechanisms as concrete workflows demand;
- continued use of existing editors and applications through capability boundaries.

**Demonstration:** optional reasoning improves an established task while deterministic memory, retrieval, and automation remain usable without it.

## E. Historical inspection and recoverable personal computing

Extend temporal foundations toward broader recovery and portability.

Outcomes:

- historical views spanning claims, decisions, permissions, and transformations;
- explicit undo or compensation where possible, with irreversible effects visible;
- alternative hypotheses or plans with provenance;
- open export schemas and deliberate migrations;
- eventual multi-device operation without surrendering local authority.

**Demonstration:** inspect how an outcome evolved and recover or branch a prior state where supported, without implying that history can reverse every external action.

## Deferred exploration

Attention scheduling, fatigue or cognitive-load inference, and speculative human-process models live in [TOOS.md](TOOS.md). Existing goal/task/context prototypes are not a mandate to expand them. Promote an idea only when a concrete user workflow, appropriate controls, and an explicit decision justify it.
