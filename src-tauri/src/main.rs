use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
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

#[derive(Debug, Serialize)]
struct SimpleMessage {
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetupConfig {
    brainer_root: Option<String>,
    workspace_root: String,
    default_workspace: Option<String>,
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

fn build_env_content(config: &SetupConfig, host_home: &str) -> String {
    let workspace_root = config.workspace_root.trim();
    let ollama_model = config.ollama_model.clone().unwrap_or_default();

    let mut lines = vec![
        "# Generated by Brainer App wizard".to_string(),
        format!("HOST_HOME={}", host_home),
        format!("HOST_WORKSPACES_DIR={}", workspace_root),
        format!("HOST_CODEX_DIR={}/.codex", host_home),
        format!("HOST_CURSOR_DIR={}/.cursor", host_home),
        format!(
            "HOST_CLAUDE_DIR={}/Library/Application Support/Claude",
            host_home
        ),
        format!("HOST_ANTIGRAVITY_DIR={}/.gemini/antigravity", host_home),
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

    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline {
        if let Ok(resp) = client.get(format!("{}/api/runtime/diagnostics", BACKEND_URL)).send() {
            if resp.status().is_success() {
                return Ok(());
            }
        }
        thread::sleep(Duration::from_secs(2));
    }

    Err("Backend did not become ready in time.".to_string())
}

fn post_initial_settings(config: &SetupConfig) -> Result<(), String> {
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
        Ok(())
    } else {
        let body = response.text().unwrap_or_default();
        Err(format!(
            "Settings API returned {}: {}",
            status,
            trim_output(&body)
        ))
    }
}

fn register_default_workspace(default_workspace: &str) -> Result<(), String> {
    let body = serde_json::json!({
        "name": default_workspace,
        "source_path": null,
        "is_active": true
    });

    let client = Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|err| format!("Failed to build HTTP client: {}", err))?;

    let response = client
        .post(format!("{}/api/workspaces/registry", BACKEND_URL))
        .json(&body)
        .send()
        .map_err(|err| format!("Failed to register workspace: {}", err))?;

    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        let body = response.text().unwrap_or_default();
        Err(format!(
            "Workspace registry API returned {}: {}",
            status,
            trim_output(&body)
        ))
    }
}

#[tauri::command]
fn detect_brainer_root() -> Result<String, String> {
    find_brainer_root()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "Could not detect Brainer root. Set BRAINER_ROOT env or run app inside repo.".to_string())
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
            why: "Modelo general rápido y estable para tareas mixtas en Brainer.".to_string(),
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

    let env_content = build_env_content(&config, &host_home);
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
    logs.push("Backend diagnostics endpoint is reachable.".to_string());

    match post_initial_settings(&config) {
        Ok(_) => logs.push("Saved initial runtime/provider settings via API.".to_string()),
        Err(err) => logs.push(format!("Warning: could not persist settings via API: {}", err)),
    }

    if let Some(default_ws) = &config.default_workspace {
        if !default_ws.trim().is_empty() {
            match register_default_workspace(default_ws.trim()) {
                Ok(_) => logs.push(format!("Registered and activated workspace '{}'.", default_ws.trim())),
                Err(err) => logs.push(format!("Warning: failed to register default workspace: {}", err)),
            }
        }
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

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            detect_brainer_root,
            run_preflight_checks,
            discover_workspace_children,
            get_ollama_models,
            get_recommended_models,
            pull_ollama_model,
            delete_ollama_model,
            apply_setup,
            install_agent_mcp,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Brainer app");
}
