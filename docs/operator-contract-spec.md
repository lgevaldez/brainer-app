# Brainer Operator Contract Spec

## Objective
Brainer App is the desktop operator for Brainer.
It should make setup, orchestration, AGENTS generation, indexing, and future cognitive workflows operable without weakening Brainer as the source of truth.

## Role Boundaries
- Brainer backend is the brain.
- Brainer App is the operator console.
- MCP and HTTP API are the transport surfaces for cognitive exchange.

Brainer App should orchestrate, validate, and present state.
It should not become the place where core cognitive logic lives.

## Operator Responsibilities
- detect and validate the Brainer backend,
- guide environment setup,
- trigger reindex flows,
- generate AGENTS contracts,
- surface progress and failures clearly,
- trigger future background cognition controls when they exist.

## AGENTS Generation Responsibilities
Generated AGENTS files should teach agents to:
- start with Brainer before broad local scanning,
- retrieve memory and context first,
- send back high-signal memory during execution,
- checkpoint before handoff,
- scope memory to workspace, project, and session whenever possible.

## Workspace Discovery Requirements
Workspace discovery should prefer valid project-like targets over arbitrary folders.
The operator should help users avoid generating AGENTS or triggering reindex for irrelevant directories.

## Mental Process Alignment
Brainer App should prepare operator workflows for future background cognition:
- index-triggered consolidation,
- post-task checkpoint prompts,
- memory promotion review,
- visibility into what Brainer considered important.

## Non-Goals
- Duplicating backend memory logic in the desktop app.
- Making AGENTS generation the only source of protocol truth.
- Treating Brainer as language-specific or tied to one agent client.
