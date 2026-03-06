use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use tauri::Emitter;
use serde::{Deserialize, Serialize};

const BACKEND_URL: &str = "http://127.0.0.1:8000";

#[derive(Debug, Serialize)]
struct CommandOutput {
    status: i32,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Serialize)]
struct PreflightStatus {
    docker_cli: bool,
    docker_compose: bool,
    docker_daemon: bool,
    ollama_cli: bool,
    ollama_running: bool,
}

#[derive(Debug, Serialize)]
struct FolderEntry {
    name: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct ModelSuggestion {
    name: String,
    why: String,
}

#[derive(Debug, Serialize)]
struct SetupResult {
    env_file: String,
    logs: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
struct SetupProgressEvent {
    progress: u8,
    stage: String,
    message: String,
}

#[derive(Debug, Serialize, Clone)]
struct AgentsIndexProgressEvent {
    progress: u8,
    stage: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct SimpleMessage {
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetupConfig {
    brainer_root: Option<String>,
    workspace_root: String,
    github_token: Option<String>,
    github_webhook_secret: Option<String>,
    github_webhook_url: Option<String>,
    ollama_model: Option<String>,
    allow_insecure_webhooks: bool,
}

#[derive(Debug, Serialize)]
struct SettingPayload {
    items: Vec<SettingItem>,
}

#[derive(Debug, Serialize)]
struct SettingItem {
    key: String,
    value: serde_json::Value,
    is_secret: bool,
}

fn trim_output(text: &str) -> String {
    const MAX: usize = 2000;
    if text.len() <= MAX {
        text.to_string()
    } else {
        format!("{}... [trimmed]", &text[..MAX])
    }
}

fn run_command(program: &str, args: &[&str], cwd: Option<&Path>) -> Result<CommandOutput, String> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }

    let output = command
        .output()
        .map_err(|err| format!("Failed to run {} {:?}: {}", program, args, err))?;

    Ok(CommandOutput {
        status: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

fn is_brainer_root(path: &Path) -> bool {
    path.join("docker-compose.yml").is_file() && path.join("backend").is_dir()
}

fn normalize_existing_dir(path: PathBuf) -> Option<PathBuf> {
    if !path.exists() || !path.is_dir() {
        return None;
    }
    path.canonicalize().ok()
}

fn find_brainer_root() -> Option<PathBuf> {
    if let Ok(raw) = env::var("BRAINER_ROOT") {
        if let Some(path) = normalize_existing_dir(PathBuf::from(raw)) {
            if is_brainer_root(&path) {
                return Some(path);
            }
        }
    }

    let mut candidates = vec![];
    if let Ok(cwd) = env::current_dir() {
        if let Some(path) = normalize_existing_dir(cwd) {
            candidates.push(path);
        }
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            if let Some(path) = normalize_existing_dir(parent.to_path_buf()) {
                candidates.push(path);
            }
        }
    }

    let sibling_names = ["brainer", "Brainer", "brainer-backend"];
    let mut visited = HashSet::new();

    for base in candidates {
        let base_key = base.to_string_lossy().to_string();
        if !visited.insert(base_key) {
            continue;
        }

        if is_brainer_root(&base) {
            return Some(base);
        }

        for ancestor in base.ancestors() {
            let candidate = ancestor.to_path_buf();
            if is_brainer_root(&candidate) {
                return Some(candidate);
            }

            for sibling in sibling_names {
                let sibling_candidate = candidate.join(sibling);
                if let Some(normalized) = normalize_existing_dir(sibling_candidate) {
                    if is_brainer_root(&normalized) {
                        return Some(normalized);
                    }
                }
            }

            if let Ok(entries) = fs::read_dir(&candidate) {
                for entry in entries.flatten().take(30) {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if !name.contains("brainer") {
                        continue;
                    }
                    if let Some(normalized) = normalize_existing_dir(entry.path()) {
                        if is_brainer_root(&normalized) {
                            return Some(normalized);
                        }
                    }
                }
            }
        }
    }

    None
}

fn ensure_workspace_root(path: &str) -> Result<PathBuf, String> {
    let root = PathBuf::from(path);
    if !root.exists() {
        return Err(format!("Workspace root does not exist: {}", path));
    }
    if !root.is_dir() {
        return Err(format!("Workspace root is not a directory: {}", path));
    }
    root.canonicalize()
        .map_err(|err| format!("Unable to canonicalize workspace root: {}", err))
}

fn build_env_content(config: &SetupConfig, host_home: &str, brainer_root: &Path) -> String {
    let workspace_root = config.workspace_root.trim();
    let ollama_model = config.ollama_model.clone().unwrap_or_default();

    let mut lines = vec![
        "# Generated by Brainer Operator wizard".to_string(),
        format!("HOST_HOME={}", host_home),
        format!("HOST_WORKSPACES_DIR={}", workspace_root),
        "WORKSPACES_MOUNT_MODE=rw".to_string(),
        format!("HOST_CODEX_DIR={}/.codex", host_home),
        format!("HOST_CURSOR_DIR={}/.cursor", host_home),
        format!(
            "HOST_CLAUDE_DIR={}/Library/Application Support/Claude",
            host_home
        ),
        format!("HOST_ANTIGRAVITY_DIR={}/.gemini/antigravity", host_home),
        format!("BRAINER_PROJECT_ROOT={}", brainer_root.to_string_lossy()),
        format!("OLLAMA_MODEL={}", ollama_model),
        format!(
            "BRAINER_ALLOW_INSECURE_WEBHOOKS={}",
            if config.allow_insecure_webhooks { "true" } else { "false" }
        ),
        format!("GITHUB_TOKEN={}", config.github_token.clone().unwrap_or_default()),
        format!(
            "GITHUB_WEBHOOK_SECRET={}",
            config.github_webhook_secret.clone().unwrap_or_default()
        ),
        format!(
            "GITHUB_WEBHOOK_URL={}",
            config.github_webhook_url.clone().unwrap_or_default()
        ),
    ];

    lines.push(String::new());
    lines.join("\n")
}

fn wait_for_backend_ready() -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(4))
        .build()
        .map_err(|err| format!("Failed to build HTTP client: {}", err))?;

    let probes = [
        format!("{}/api/health", BACKEND_URL),
        format!("{}/openapi.json", BACKEND_URL),
        format!("{}/api/installers/status", BACKEND_URL),
    ];

    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline {
        for url in &probes {
            if let Ok(resp) = client.get(url).send() {
                if resp.status().is_success() {
                    return Ok(());
                }
            }
        }
        thread::sleep(Duration::from_secs(2));
    }

    Err("Backend did not become ready in time.".to_string())
}

fn post_initial_settings(config: &SetupConfig) -> Result<bool, String> {
    let mut items = vec![];

    if let Some(model) = &config.ollama_model {
        if !model.trim().is_empty() {
            items.push(SettingItem {
                key: "OLLAMA_MODEL".to_string(),
                value: serde_json::Value::String(model.clone()),
                is_secret: false,
            });
        }
    }

    if let Some(token) = &config.github_token {
        if !token.trim().is_empty() {
            items.push(SettingItem {
                key: "GITHUB_TOKEN".to_string(),
                value: serde_json::Value::String(token.clone()),
                is_secret: true,
            });
        }
    }

    if let Some(secret) = &config.github_webhook_secret {
        if !secret.trim().is_empty() {
            items.push(SettingItem {
                key: "GITHUB_WEBHOOK_SECRET".to_string(),
                value: serde_json::Value::String(secret.clone()),
                is_secret: true,
            });
        }
    }

    if let Some(url) = &config.github_webhook_url {
        if !url.trim().is_empty() {
            items.push(SettingItem {
                key: "GITHUB_WEBHOOK_URL".to_string(),
                value: serde_json::Value::String(url.clone()),
                is_secret: false,
            });
        }
    }

    items.push(SettingItem {
        key: "BRAINER_ALLOW_INSECURE_WEBHOOKS".to_string(),
        value: serde_json::Value::Bool(config.allow_insecure_webhooks),
        is_secret: false,
    });

    let payload = SettingPayload { items };
    let client = Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|err| format!("Failed to build HTTP client: {}", err))?;

    let response = client
        .put(format!("{}/api/settings/", BACKEND_URL))
        .json(&payload)
        .send()
        .map_err(|err| format!("Failed to save settings: {}", err))?;

    let status = response.status();
    if status.is_success() {
        Ok(true)
    } else if status.as_u16() == 404 {
        // API-only mode may not expose /api/settings. .env remains source of truth.
        Ok(false)
    } else {
        let body = response.text().unwrap_or_default();
        Err(format!(
            "Settings API returned {}: {}",
            status,
            trim_output(&body)
        ))
    }
}

fn emit_setup_progress(app: &tauri::AppHandle, progress: u8, stage: &str, message: &str) {
    let payload = SetupProgressEvent {
        progress,
        stage: stage.to_string(),
        message: message.to_string(),
    };
    let _ = app.emit("setup-progress", payload);
}

fn emit_agents_index_progress(app: &tauri::AppHandle, progress: u8, stage: &str, message: &str) {
    let payload = AgentsIndexProgressEvent {
        progress,
        stage: stage.to_string(),
        message: message.to_string(),
    };
    let _ = app.emit("agents-index-progress", payload);
}

#[tauri::command]
fn detect_brainer_root() -> Result<String, String> {
    find_brainer_root()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "Could not detect Brainer root. Set BRAINER_ROOT env or run the operator inside a compatible workspace.".to_string())
}

#[tauri::command]
fn run_preflight_checks() -> PreflightStatus {
    let docker_cli = run_command("docker", &["--version"], None)
        .map(|out| out.status == 0)
        .unwrap_or(false);
    let docker_compose = run_command("docker", &["compose", "version"], None)
        .map(|out| out.status == 0)
        .unwrap_or(false);
    let docker_daemon = run_command("docker", &["info"], None)
        .map(|out| out.status == 0)
        .unwrap_or(false);
    let ollama_cli = run_command("ollama", &["--version"], None)
        .map(|out| out.status == 0)
        .unwrap_or(false);
    let ollama_running = run_command("ollama", &["list"], None)
        .map(|out| out.status == 0)
        .unwrap_or(false);

    PreflightStatus {
        docker_cli,
        docker_compose,
        docker_daemon,
        ollama_cli,
        ollama_running,
    }
}

#[tauri::command]
fn discover_workspace_children(root_path: String) -> Result<Vec<FolderEntry>, String> {
    let path = ensure_workspace_root(&root_path)?;

    let mut entries = vec![];
    let dir = fs::read_dir(path).map_err(|err| format!("Failed to read workspace root: {}", err))?;
    for entry in dir {
        let entry = entry.map_err(|err| format!("Read dir entry error: {}", err))?;
        let child_path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.starts_with('.') {
            continue;
        }
        if child_path.is_dir() {
            entries.push(FolderEntry {
                name: file_name,
                path: child_path.to_string_lossy().to_string(),
            });
        }
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

fn render_master_root_agents_template(root_name: &str, workspace_names: &[String]) -> String {
    let workspace_block = if workspace_names.is_empty() {
        "- none detected yet".to_string()
    } else {
        workspace_names
            .iter()
            .map(|name| format!("- `{}`", name))
            .collect::<Vec<_>>()
            .join("
")
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
8. `get_graph_dependencies` when structural impact matters
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

#[tauri::command]
fn generate_master_root_agents_template(
    workspace_root: String,
    workspace_names: Vec<String>,
    overwrite: bool,
) -> Result<serde_json::Value, String> {
    let root = ensure_workspace_root(&workspace_root)?;
    let root_name = root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("workspace-root");

    let mut unique_workspaces = workspace_names
        .into_iter()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    unique_workspaces.sort();
    unique_workspaces.dedup();

    let agents_path = root.join("AGENTS.md");
    let existed_before = agents_path.exists();
    if existed_before && !overwrite {
        return Ok(serde_json::json!({
            "status": "ok",
            "kind": "master_root",
            "path": agents_path.to_string_lossy().to_string(),
            "file_status": "skipped_exists",
            "workspace_count": unique_workspaces.len(),
        }));
    }

    let content = render_master_root_agents_template(root_name, &unique_workspaces);
    fs::write(&agents_path, content)
        .map_err(|err| format!("Failed writing {}: {}", agents_path.display(), err))?;

    Ok(serde_json::json!({
        "status": "ok",
        "kind": "master_root",
        "path": agents_path.to_string_lossy().to_string(),
        "file_status": if existed_before { "updated" } else { "created" },
        "workspace_count": unique_workspaces.len(),
    }))
}

#[tauri::command]
fn get_ollama_models() -> Result<Vec<String>, String> {
    let output = run_command("ollama", &["list"], None)?;
    if output.status != 0 {
        return Err(trim_output(&output.stderr));
    }

    let mut models = vec![];
    let mut seen = HashSet::new();
    for (idx, line) in output.stdout.lines().enumerate() {
        if idx == 0 && line.to_ascii_lowercase().contains("name") {
            continue;
        }
        let first = line.split_whitespace().next().unwrap_or_default().trim();
        if first.is_empty() {
            continue;
        }
        if seen.insert(first.to_string()) {
            models.push(first.to_string());
        }
    }

    models.sort();
    Ok(models)
}

#[tauri::command]
fn get_recommended_models() -> Vec<ModelSuggestion> {
    vec![
        ModelSuggestion {
            name: "qwen2.5-coder:7b".to_string(),
            why: "Balance sólido entre velocidad y calidad para código local.".to_string(),
        },
        ModelSuggestion {
            name: "llama3.1:8b".to_string(),
            why: "Modelo general rápido y estable para operar Brainer y tareas mixtas.".to_string(),
        },
        ModelSuggestion {
            name: "deepseek-r1:8b".to_string(),
            why: "Buena capacidad de razonamiento manteniendo costo local razonable.".to_string(),
        },
    ]
}

#[tauri::command]
fn pull_ollama_model(model_name: String) -> Result<CommandOutput, String> {
    if model_name.trim().is_empty() {
        return Err("Model name is required.".to_string());
    }
    run_command("ollama", &["pull", model_name.trim()], None)
}

#[tauri::command]
fn delete_ollama_model(model_name: String) -> Result<CommandOutput, String> {
    if model_name.trim().is_empty() {
        return Err("Model name is required.".to_string());
    }
    run_command("ollama", &["rm", model_name.trim()], None)
}

#[tauri::command]
fn apply_setup(config: SetupConfig) -> Result<SetupResult, String> {
    let mut logs = vec![];
    let workspace_root = ensure_workspace_root(&config.workspace_root)?;

    let brainer_root = if let Some(raw_root) = &config.brainer_root {
        let path = PathBuf::from(raw_root);
        if path.join("docker-compose.yml").exists() && path.join("backend").is_dir() {
            path
        } else {
            return Err("Provided brainerRoot is invalid; docker-compose.yml/backend not found.".to_string());
        }
    } else {
        find_brainer_root().ok_or_else(|| "Could not detect Brainer root folder.".to_string())?
    };

    let host_home = env::var("HOME").unwrap_or_else(|_| "/Users/unknown".to_string());
    let mut config = config;
    config.workspace_root = workspace_root.to_string_lossy().to_string();

    let env_content = build_env_content(&config, &host_home, &brainer_root);
    let env_path = brainer_root.join(".env");
    fs::write(&env_path, env_content)
        .map_err(|err| format!("Failed writing {}: {}", env_path.display(), err))?;
    logs.push(format!("Wrote {}", env_path.display()));

    let build_output = run_command(
        "docker",
        &["compose", "build", "backend"],
        Some(&brainer_root),
    )?;
    logs.push(format!("docker compose build backend -> status {}", build_output.status));
    if !build_output.stdout.is_empty() {
        logs.push(format!("build stdout: {}", trim_output(&build_output.stdout)));
    }
    if build_output.status != 0 {
        logs.push(format!("build stderr: {}", trim_output(&build_output.stderr)));
        return Err("docker compose build backend failed.".to_string());
    }

    let up_output = run_command("docker", &["compose", "up", "-d"], Some(&brainer_root))?;
    logs.push(format!("docker compose up -d -> status {}", up_output.status));
    if !up_output.stdout.is_empty() {
        logs.push(format!("up stdout: {}", trim_output(&up_output.stdout)));
    }
    if up_output.status != 0 {
        logs.push(format!("up stderr: {}", trim_output(&up_output.stderr)));
        return Err("docker compose up -d failed.".to_string());
    }

    wait_for_backend_ready()?;
    logs.push("Backend health endpoint is reachable.".to_string());

    match post_initial_settings(&config) {
        Ok(true) => logs.push("Saved initial runtime/provider settings via API.".to_string()),
        Ok(false) => logs.push("Settings API not available; using .env values as source of truth.".to_string()),
        Err(err) => logs.push(format!("Warning: could not persist settings via API: {}", err)),
    }

    Ok(SetupResult {
        env_file: env_path.to_string_lossy().to_string(),
        logs,
    })
}

#[tauri::command]
fn install_agent_mcp(agent: String) -> Result<SimpleMessage, String> {
    let clean = agent.trim().to_lowercase();
    let allowed = ["codex", "cursor", "claude", "antigravity"];
    if !allowed.contains(&clean.as_str()) {
        return Err(format!("Unsupported agent '{}'.", clean));
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|err| format!("Failed to build HTTP client: {}", err))?;

    let response = client
        .post(format!("{}/api/installers/install/{}", BACKEND_URL, clean))
        .send()
        .map_err(|err| format!("Failed to call installer endpoint: {}", err))?;

    let status = response.status();
    let body = response.text().unwrap_or_default();
    if status.is_success() {
        Ok(SimpleMessage {
            message: format!("Installed MCP for {}", clean),
        })
    } else {
        Err(format!(
            "Installer API returned {}: {}",
            status,
            trim_output(&body)
        ))
    }
}


#[tauri::command]
async fn reindex_workspace_with_progress(
    app: tauri::AppHandle,
    workspace_name: String,
    force: bool,
) -> Result<SimpleMessage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        reindex_workspace_with_progress_blocking(app, workspace_name, force)
    })
    .await
    .map_err(|err| format!("Reindex worker join error: {}", err))?
}

fn reindex_workspace_with_progress_blocking(
    app: tauri::AppHandle,
    workspace_name: String,
    force: bool,
) -> Result<SimpleMessage, String> {
    let workspace = workspace_name.trim().to_string();
    if workspace.is_empty() {
        return Err("workspace_name is required".to_string());
    }

    let mode = if force { "force" } else { "incremental" };
    emit_agents_index_progress(
        &app,
        2,
        "prepare",
        &format!("Preparing {} index for workspace '{}'...", mode, workspace),
    );

    let client = Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|err| format!("Failed to build HTTP client: {}", err))?;

    let trigger_url = format!(
        "{}/api/workspaces/{}/index?force={}",
        BACKEND_URL,
        workspace,
        if force { "true" } else { "false" }
    );

    let trigger_response = client
        .post(trigger_url)
        .send()
        .map_err(|err| format!("Failed to trigger workspace index: {}", err))?;

    if !trigger_response.status().is_success() {
        let status = trigger_response.status();
        let body = trigger_response.text().unwrap_or_default();
        emit_agents_index_progress(&app, 100, "failed", "Failed to start workspace indexing.");
        return Err(format!(
            "Workspace index trigger failed {}: {}",
            status,
            trim_output(&body)
        ));
    }

    emit_agents_index_progress(
        &app,
        5,
        "queued",
        &format!("{} indexing started for '{}'.", mode, workspace),
    );

    let status_url = format!("{}/api/workspaces/{}/status", BACKEND_URL, workspace);
    let timeout_secs = if force { 3600 } else { 1800 };
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let mut last_signature = String::new();

    while Instant::now() < deadline {
        let response = client
            .get(&status_url)
            .send()
            .map_err(|err| format!("Failed polling index status: {}", err))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            emit_agents_index_progress(&app, 100, "failed", "Index status polling failed.");
            return Err(format!(
                "Workspace status API returned {}: {}",
                status,
                trim_output(&body)
            ));
        }

        let payload = response
            .json::<serde_json::Value>()
            .map_err(|err| format!("Invalid JSON in workspace status response: {}", err))?;

        let stage = payload
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let progress_raw = payload.get("progress").and_then(|v| v.as_u64()).unwrap_or(0);
        let progress = std::cmp::min(progress_raw, 100) as u8;
        let current = payload.get("current").and_then(|v| v.as_u64()).unwrap_or(0);
        let total = payload.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        let message = payload
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        let details = if !message.is_empty() {
            message
        } else if total > 0 {
            format!("{}/{} files processed", current, total)
        } else {
            format!("Status: {}", stage)
        };

        let signature = format!("{}:{}:{}", progress, stage, details);
        if signature != last_signature {
            emit_agents_index_progress(&app, progress, &stage, &details);
            last_signature = signature;
        }

        if stage == "complete" {
            emit_agents_index_progress(&app, 100, "complete", "Indexing completed.");
            return Ok(SimpleMessage {
                message: format!("{} reindex completed for '{}'.", mode, workspace),
            });
        }

        if stage == "error" {
            emit_agents_index_progress(&app, 100, "failed", "Indexing failed.");
            return Err(format!("Workspace indexing failed: {}", details));
        }

        thread::sleep(Duration::from_secs(2));
    }

    emit_agents_index_progress(&app, 100, "failed", "Indexing timed out.");
    Err(format!("Workspace indexing timed out after {} seconds.", timeout_secs))
}

#[tauri::command]
fn get_agents_suggestions(workspace_name: String) -> Result<serde_json::Value, String> {
    let workspace = workspace_name.trim();
    if workspace.is_empty() {
        return Err("workspace_name is required".to_string());
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|err| format!("Failed to build HTTP client: {}", err))?;

    let response = client
        .get(format!(
            "{}/api/workspaces/{}/agents/suggestions",
            BACKEND_URL, workspace
        ))
        .send()
        .map_err(|err| format!("Failed to call agents suggestions endpoint: {}", err))?;

    let status = response.status();
    if status.is_success() {
        response
            .json::<serde_json::Value>()
            .map_err(|err| format!("Invalid JSON in suggestions response: {}", err))
    } else {
        let body = response.text().unwrap_or_default();
        Err(format!(
            "Agents suggestions API returned {}: {}",
            status,
            trim_output(&body)
        ))
    }
}

#[tauri::command]
fn generate_agents_templates(
    workspace_name: String,
    include_project_mirrors: bool,
    overwrite: bool,
) -> Result<serde_json::Value, String> {
    let workspace = workspace_name.trim();
    if workspace.is_empty() {
        return Err("workspace_name is required".to_string());
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|err| format!("Failed to build HTTP client: {}", err))?;

    let url = format!(
        "{}/api/workspaces/{}/agents/generate?mode=auto&include_project_mirrors={}&overwrite={}",
        BACKEND_URL,
        workspace,
        if include_project_mirrors { "true" } else { "false" },
        if overwrite { "true" } else { "false" }
    );

    let response = client
        .post(url)
        .send()
        .map_err(|err| format!("Failed to call agents generate endpoint: {}", err))?;

    let status = response.status();
    if status.is_success() {
        response
            .json::<serde_json::Value>()
            .map_err(|err| format!("Invalid JSON in generate response: {}", err))
    } else {
        let body = response.text().unwrap_or_default();
        Err(format!(
            "Agents generate API returned {}: {}",
            status,
            trim_output(&body)
        ))
    }
}



#[tauri::command]
async fn apply_setup_with_progress(app: tauri::AppHandle, config: SetupConfig) -> Result<SetupResult, String> {
    tauri::async_runtime::spawn_blocking(move || apply_setup_with_progress_blocking(app, config))
        .await
        .map_err(|err| format!("Setup worker join error: {}", err))?
}

fn apply_setup_with_progress_blocking(app: tauri::AppHandle, config: SetupConfig) -> Result<SetupResult, String> {
    let mut logs = vec![];
    emit_setup_progress(&app, 2, "prepare", "Validating workspace and brainer root...");

    let workspace_root = match ensure_workspace_root(&config.workspace_root) {
        Ok(path) => path,
        Err(err) => {
            emit_setup_progress(&app, 100, "failed", "Invalid workspace root.");
            return Err(err);
        }
    };

    let brainer_root = if let Some(raw_root) = &config.brainer_root {
        let path = PathBuf::from(raw_root);
        if path.join("docker-compose.yml").exists() && path.join("backend").is_dir() {
            path
        } else {
            emit_setup_progress(&app, 100, "failed", "Provided Brainer root is invalid.");
            return Err("Provided brainerRoot is invalid; docker-compose.yml/backend not found.".to_string());
        }
    } else {
        match find_brainer_root() {
            Some(path) => path,
            None => {
                emit_setup_progress(&app, 100, "failed", "Could not detect brainer root.");
                return Err("Could not detect Brainer root folder.".to_string());
            }
        }
    };

    let host_home = env::var("HOME").unwrap_or_else(|_| "/Users/unknown".to_string());
    let mut config = config;
    config.workspace_root = workspace_root.to_string_lossy().to_string();

    emit_setup_progress(&app, 12, "env", "Writing .env configuration...");
    let env_content = build_env_content(&config, &host_home, &brainer_root);
    let env_path = brainer_root.join(".env");
    if let Err(err) = fs::write(&env_path, env_content) {
        emit_setup_progress(&app, 100, "failed", "Failed writing .env file.");
        return Err(format!("Failed writing {}: {}", env_path.display(), err));
    }
    logs.push(format!("Wrote {}", env_path.display()));

    emit_setup_progress(&app, 30, "build", "Running docker compose build backend...");
    let build_output = match run_command("docker", &["compose", "build", "backend"], Some(&brainer_root)) {
        Ok(out) => out,
        Err(err) => {
            emit_setup_progress(&app, 100, "failed", "Docker build command failed to start.");
            return Err(err);
        }
    };

    logs.push(format!("docker compose build backend -> status {}", build_output.status));
    if !build_output.stdout.is_empty() {
        logs.push(format!("build stdout: {}", trim_output(&build_output.stdout)));
    }

    if build_output.status != 0 {
        logs.push(format!("build stderr: {}", trim_output(&build_output.stderr)));
        emit_setup_progress(&app, 100, "failed", "docker compose build backend failed.");
        return Err("docker compose build backend failed.".to_string());
    }

    emit_setup_progress(&app, 72, "build_done", "Docker backend image built.");
    emit_setup_progress(&app, 78, "up", "Running docker compose up -d...");

    let up_output = match run_command("docker", &["compose", "up", "-d"], Some(&brainer_root)) {
        Ok(out) => out,
        Err(err) => {
            emit_setup_progress(&app, 100, "failed", "Docker up command failed to start.");
            return Err(err);
        }
    };

    logs.push(format!("docker compose up -d -> status {}", up_output.status));
    if !up_output.stdout.is_empty() {
        logs.push(format!("up stdout: {}", trim_output(&up_output.stdout)));
    }

    if up_output.status != 0 {
        logs.push(format!("up stderr: {}", trim_output(&up_output.stderr)));
        emit_setup_progress(&app, 100, "failed", "docker compose up -d failed.");
        return Err("docker compose up -d failed.".to_string());
    }

    emit_setup_progress(&app, 88, "up_done", "Containers started. Waiting for backend...");
    emit_setup_progress(&app, 92, "health", "Checking backend readiness...");

    if let Err(err) = wait_for_backend_ready() {
        emit_setup_progress(&app, 100, "failed", "Backend did not become ready in time.");
        return Err(err);
    }
    logs.push("Backend health endpoint is reachable.".to_string());

    emit_setup_progress(&app, 95, "settings", "Persisting initial settings...");
    match post_initial_settings(&config) {
        Ok(true) => logs.push("Saved initial runtime/provider settings via API.".to_string()),
        Ok(false) => logs.push("Settings API not available; using .env values as source of truth.".to_string()),
        Err(err) => logs.push(format!("Warning: could not persist settings via API: {}", err)),
    }

    emit_setup_progress(&app, 100, "done", "brainer is ready.");

    Ok(SetupResult {
        env_file: env_path.to_string_lossy().to_string(),
        logs,
    })
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            detect_brainer_root,
            run_preflight_checks,
            discover_workspace_children,
            generate_master_root_agents_template,
            get_ollama_models,
            get_recommended_models,
            pull_ollama_model,
            delete_ollama_model,
            apply_setup,
            apply_setup_with_progress,
            install_agent_mcp,
            reindex_workspace_with_progress,
            get_agents_suggestions,
            generate_agents_templates,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Brainer app");
}
