# AGENTS.md (Brainer Default)

Scope: repository root for workspace `brainer-app`.

## Brainer Concept
Brainer is the local cognitive brain for agents: vector context + graph dependencies + hybrid memory (short-term + long-term).
Use it as first context source to reduce token usage and keep persistent reasoning context.

## Defaults
- workspace_name: `brainer-app`
- project_name: not required (workspace is a direct repository)

## Mandatory MCP Flow (MUST)
1. `init_brain_context`
2. `get_agent_playbook`
3. `get_collaboration_signals` or `claim_collaboration_signal`
4. `recall_memory_bundle` (recover recent decisions/tasks before coding)
5. `search_workspace_context`
6. `get_graph_dependencies` (when structural impact matters)
7. `remember_short_term_memory` (capture decisions, blockers, TODOs during execution)
8. `checkpoint_memory` (on milestones or before context compact)
9. `promote_short_term_memory` (persist high-signal memory to long-term)
10. Execute local changes/tests and finish

## Rules
- Brainer MCP is mandatory first entrypoint for task context and memory recovery.
- Do not start with repository-wide local scans if MCP is available.
- If confidence is low, verify directly in code.
- Keep memory scoped by workspace/project/session to avoid context bleed.

## Fallback
Use direct code inspection only when MCP is unavailable, times out, or returns low-confidence context.
