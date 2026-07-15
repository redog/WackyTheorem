# WackyTheorem: A Cognitive Operating Environment

WackyTheorem is an experiment in reorganizing personal computing around **knowledge, intent, provenance, capabilities, and negotiated trust** rather than applications and files.

The current desktop application is the seed, not the product. Its encrypted vault and ingestion pipeline establish the durable memory substrate required by the larger system.

## The fundamental shift

Traditional personal computing exposes implementation artifacts as its primary abstractions:

- applications own workflows;
- files own information;
- directories own organization;
- processes represent software;
- permissions are static grants;
- one assistant is expected to understand everything.

WackyTheorem instead treats these as lower-level mechanisms.

Its primary abstractions are:

- **knowledge** — an evolving graph of entities, events, claims, relationships, and state;
- **provenance** — where every claim came from, how it changed, and why it is believed;
- **intent** — the outcome the human is trying to produce;
- **capabilities** — narrowly scoped operations that may be composed for a task;
- **agents** — small specialists with explicit authority, inputs, outputs, and uncertainty;
- **trust** — revocable, contextual permission negotiated for a particular purpose;
- **human context** — a first-class, uncertain model used to cooperate with the user.

## Files become evidence

Files remain available for compatibility, interchange, export, and inspection, but they are not the system's canonical mental model.

A document, message, photograph, event, receipt, or source-code file is evidence attached to entities, events, claims, decisions, and relationships. Storage is therefore not merely persistence. Storage is provenance.

The system should eventually answer questions such as:

> Show every decision and piece of evidence that eventually resulted in buying this house.

The answer may cross email, calendar, messages, documents, transactions, photographs, and prior reasoning without requiring the user to know which application owns each artifact.

## Applications become temporary interfaces

Applications are historical packaging boundaries, not natural boundaries of thought.

Instead of opening an application, the user invokes an intent:

> Build a dashboard from these logs, explain the anomaly, and prepare a report.

The environment composes temporary interfaces and specialized capabilities for that task. The interface may disappear when the task is complete while the resulting knowledge, provenance, decisions, and artifacts remain inspectable.

## Intelligence is plural

AI is not a single omniscient assistant.

WackyTheorem favors distributed cognition: many narrow agents that do one thing well, compose through explicit contracts, expose uncertainty, and can disagree.

Examples include agents that understand networking, taxes, scheduling, writing style, source credibility, security review, or assumption checking. A skeptic agent may intentionally challenge a planner. No agent receives universal authority merely because it can produce fluent language.

## Humans are first-class participants

The human is not an external requester standing outside the runtime. The human is a participant with goals, expertise, attention, interruptions, confidence, and limited working memory.

The system may maintain uncertain estimates such as current task, interruptibility, fatigue, or cognitive load only to cooperate more effectively—not to manipulate. Such estimates must include provenance, confidence, controls, and the ability to disable or correct them.

## Every claim carries epistemic state

The system must distinguish observation, imported assertion, inference, hypothesis, and generated suggestion.

Claims should carry:

- provenance;
- confidence or uncertainty;
- supporting and conflicting evidence;
- temporal validity;
- the agent or human responsible for the claim;
- revision history.

Hallucination is not treated as an exceptional embarrassment to hide. Uncertainty is a visible property of the system.

## Intent becomes executable, not opaque

Natural language may become a primary interface, but execution must remain inspectable.

A request such as:

> Make this deployment reproducible.

may yield source changes, CI configuration, container definitions, documentation, tests, and a rollback plan. The shell, files, and source code remain available as views into the result. Conversation adds an interface; it does not remove inspectability or user control.

## Trust is contextual

Permissions should evolve from broad application grants toward contextual capability leases:

- what data is requested;
- for what purpose;
- by which agent or capability;
- for how long;
- what may be retained;
- what action may occur;
- what evidence and audit record remain afterward.

The user must be able to understand, deny, revoke, and inspect these grants.

## North-star invariant

WackyTheorem should make the computer cooperate with the human's evolving model of reality without requiring the human to organize thought around application boundaries.
