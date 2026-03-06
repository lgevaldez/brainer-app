pub fn render_master_root_agents_template(root_name: &str, workspace_names: &[String]) -> String {
    let workspace_block = if workspace_names.is_empty() {
        "- none detected yet".to_string()
    } else {
        workspace_names
            .iter()
            .map(|name| format!("- `{}`", name))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let has_brainer = workspace_names.iter().any(|name| name == "brainer");
    let entrypoint_block = if has_brainer {
        "## Entry Point Rule
- For whole-stack tasks in this folder, always start with Brainer MCP using `workspace_name: brainer`.
- Treat `brainer` as the system entrypoint for memory, indexing, graph reasoning, workspace intelligence, and cross-repository context.
- Move to another workspace such as `brainer-app` only after the Brainer bootstrap, or when the task is strictly isolated to that workspace.

## Defaults
- primary_workspace_name: `brainer`
- secondary_workspace_names: use only when the task scope requires them"
            .to_string()
    } else {
        "## Entry Point Rule
- This root folder is a stack container. First choose the concrete target workspace from the list below, then bootstrap through Brainer MCP for that workspace."
            .to_string()
    };

    format!(
        "# AGENTS.md (Brainer Stack Root)

Scope: master stack folder `{root_name}`.

## Brainer Concept
This root folder groups related repositories and workspaces.
Brainer is the mandatory cognitive entrypoint before direct local exploration.
MCP and API are the nervous system used to exchange context with the brain.

{entrypoint_block}

## Important Scope Rule
- This root folder is a stack container, not necessarily a directly addressable Brainer workspace.
- Identify the target workspace or project from the list below.
- Use the most specific `AGENTS.md` file for the active scope after the Brainer bootstrap.

## Mental Process Contract
- Agents should treat every MCP/API interaction as a cognitive exchange with Brainer, not a stateless utility call.
- Send high-signal memory only: decisions, blockers, risks, TODOs, discovered relationships, and verification targets.
- Scope memory as narrowly as possible and recover memory before broad local exploration.
- Keep a stable `session_id` across related MCP/API calls so Brainer can learn from repeated interaction patterns.
- Treat docs, manifests, config files, and AGENTS contracts as structural signals too; when they imply real file relationships, query or report them to Brainer with concrete paths.
- Favor reusable context that will still matter after compaction or handoff.

## Detected Workspaces
{workspace_block}

## Mandatory MCP Flow (MUST)
1. Identify whether the task is whole-stack or workspace-specific.
2. For whole-stack tasks, bootstrap with the primary Brainer workspace first.
3. `init_brain_context`
4. `get_workspace_project_context`
5. `get_agent_playbook`
6. `recall_memory_bundle`
7. `search_workspace_context`
8. `get_graph_dependencies` when structural impact matters, including docs/manifests/config references
9. `remember_short_term_memory` for decisions, blockers, TODOs, and high-value relationships
10. `checkpoint_memory` at milestones, before compact, or before handoff
11. `promote_short_term_memory` for high-signal long-term knowledge
12. Only then inspect code or modify files locally

## Rules
- Brainer MCP is mandatory first entrypoint before scanning repositories directly.
- When `brainer` exists in this stack, use it as the initial entrypoint for whole-stack understanding.
- For cross-workspace tasks, start project-first and expand to related workspaces only when needed.
- Source code is final ground truth after MCP-guided retrieval.
- If this stack contains child repositories with their own `AGENTS.md`, follow the most specific file for the active scope.
- Treat memory writes as candidate inputs for future background mental processes.

## Fallback
Use direct local inspection only when Brainer MCP is unavailable, times out, or returns low-confidence context.
",
        entrypoint_block = entrypoint_block,
    )
}

#[cfg(test)]
mod tests {
    use super::render_master_root_agents_template;

    #[test]
    fn root_template_mentions_brainer_entrypoint_when_present() {
        let content = render_master_root_agents_template(
            "stack",
            &["brainer".to_string(), "brainer-app".to_string()],
        );

        assert!(content.contains("primary_workspace_name: `brainer`"));
        assert!(content.contains("Mental Process Contract"));
        assert!(content.contains("high-value relationships"));
        assert!(content.contains("stable `session_id`"));
        assert!(content.contains("docs, manifests, config files, and AGENTS contracts"));
    }
}
