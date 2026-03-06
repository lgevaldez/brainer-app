# AGENTS.md (Brainer Default)

Scope: repository root for workspace `brainer-app`.

## Brainer Concept
Brainer is the local cognitive brain for software workspaces.
Its core job is to perceive repository state, form useful connections, preserve continuity, and return actionable context to agents.
MCP and HTTP API are the nervous system and senses used to exchange context with the brain.

## Defaults
- workspace_name: `brainer-app`
- project_name: not required (workspace is a direct repository)

## Mental Process Contract
- Ask Brainer for recall and context before wide local exploration.
- Send only high-signal memory: decisions, blockers, risks, TODOs, discovered relationships, and assumptions that must be verified later.
- Include `workspace_name`, `project_name`, and `session_id` whenever the task has that scope.
- Keep a stable `session_id` across related MCP/API calls so Brainer can learn from repeated interaction patterns.
- Treat docs, manifests, config files, and AGENTS contracts as structural signals too; when they imply real file relationships, query or report them to Brainer with concrete paths.
- Keep memory entries factual, compact, and reusable across future sessions.
- Treat memory writes as inputs for later background consolidation and long-term promotion.

## Mandatory MCP Flow (MUST)
1. `init_brain_context`
2. `get_agent_playbook`
3. `get_collaboration_signals` or `claim_collaboration_signal`
4. `recall_memory_bundle` (recover recent decisions/tasks before coding)
5. `search_workspace_context`
6. `get_graph_dependencies` (when structural impact matters, including docs/manifests/config references)
7. `remember_short_term_memory` (capture decisions, blockers, TODOs, and important relationships during execution)
8. `checkpoint_memory` (on milestones, before handoff, or before context compact)
9. `promote_short_term_memory` (persist high-signal memory to long-term)
10. Execute local changes/tests and finish

## Rules
- Brainer MCP is mandatory first entrypoint for task context and memory recovery.
- Do not start with repository-wide local scans if MCP is available.
- If confidence is low, verify directly in code.
- Keep memory scoped by workspace/project/session to avoid context bleed.
- When a conclusion is uncertain, store it as something to verify, not as established truth.

## Fallback
Use direct code inspection only when MCP is unavailable, times out, or returns low-confidence context.
