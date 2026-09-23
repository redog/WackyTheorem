# WackyTheorem: A Cognitive Operating Environment

WackyTheorem explores personal computing organized around knowledge, time, intent, provenance, capabilities, and negotiated trust. The user should be able to recover the context of a decision or act on a commitment without remembering which application holds the relevant information.

The desktop application is a starting point for this environment. Local authority, encrypted memory, and inspectable evidence make it possible for the environment to cooperate with the user while remaining under their control.

## A model that connects meaning, history, and action

- **LifeGraph is the semantic model:** entities, events, claims, relationships, and evidence describe what things mean and how they relate.
- **The temporal stream (lifestream) is the history model:** it describes what happened, when it happened, and how the system came to know it.
- **Substreams and saved queries are the organizational model:** overlapping views bring together what matters for an intent without moving or owning the underlying knowledge.
- **Capabilities are the action model:** explicit contracts describe what an operation reads, produces, and changes under negotiated authority.
- **Agents are optional reasoning participants:** they may help interpret bounded context, but memory, retrieval, organization, and deterministic automation remain useful without them.

These are complementary views of one personal knowledge system, not a requirement for separate databases or a new framework for each concept.

## Organization follows the question

Projects, collections, dashboards, and workspaces are projections over canonical knowledge and history. The same evidence can appear in several views. Creating or removing a view does not relocate or destroy its sources.

A question can become a saved, living substream:

> Show me Project Alpha: its evidence, changes, decisions, and upcoming commitments.

The user should be able to inspect earlier knowledge, narrow the view, summarize it, or watch it for a relevant change. Future commitments share this temporal model with past observations, while remaining visibly distinct from things that actually occurred.

The useful loop is:

**Capture authorized sources → preserve temporal history → derive knowledge → query or save a substream → summarize → watch, remind, or act → preserve the resulting history.**

## Files remain first-class evidence

Files, messages, photos, logs, calendar objects, and source records are source artifacts. Their original content, identity, and context may be the strongest evidence available. They remain importable, inspectable, and exportable; they are not merely compatibility baggage or the containers that own knowledge.

A derived claim or summary should lead back to its sources. An overview may be a table, chart, timeline, task list, or narrative, and must not silently replace the evidence it condenses.

## Applications stop owning the information model

Existing applications can remain excellent editors, renderers, and execution engines. WackyTheorem connects their outputs through shared knowledge, history, and capability contracts.

An intent such as “explain the anomaly in these logs and prepare a report” may compose a temporary interface. Closing that interface leaves the durable evidence, decisions, and results inspectable. The shell, source code, and ordinary tools remain available.

## Intelligence remains accountable

Prefer deterministic queries and tools where they can answer reliably. Optional models can help with interpretation and synthesis without receiving universal authority. No fixed team of runtime roles is required.

Observation, imported assertion, inference, hypothesis, and generated suggestion must remain distinguishable. Claims carry provenance, uncertainty, temporal scope, supporting or conflicting evidence, authorship, and revision relationships. Fluent language does not turn an inference into a fact.

## Trust and human agency

Authority belongs to the user. Access and action should be understandable, contextual, least-privileged, revocable, and tied to a purpose, duration, and retention policy. The user can inspect what was proposed, authorized, attempted, and changed.

Explicit goals and commitments help the environment cooperate with the user. Speculative inference about fatigue, attention, or cognition belongs in the idea incubator until a concrete workflow justifies it. Any such model must remain optional, uncertain, visible, and correctable.

## North star

WackyTheorem should help the human recover context, understand change, and carry out intent without surrendering control or organizing thought around application boundaries.

The temporal and projected-organization ideas draw on Eric Freeman and David Gelernter, *Lifestreams: A Storage Model for Personal Data*, SIGMOD Record 25(1), March 1996, pp. 80–86. The paper is a conceptual source, not a prescribed UI, network architecture, or implementation plan; see [D16](DECISIONS.md#d16-temporal-history-and-projected-organization).
