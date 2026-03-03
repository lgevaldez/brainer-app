import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

const totalSteps = 6;
let currentStep = 1;

const state = {
  brainerRoot: '',
  preflight: null,
  workspaceRoot: '',
  workspaceChildren: [],
  defaultWorkspace: '',
  githubToken: '',
  githubSecret: '',
  githubWebhookUrl: '',
  allowInsecureWebhooks: false,
  installedModels: [],
  selectedModel: '',
  suggestedModels: [],
  selectedAgents: new Set(['codex']),
  setupApplied: false,
};

const el = (id) => document.getElementById(id);

function appendLog(message) {
  const log = el('execution-log');
  const stamp = new Date().toISOString();
  log.textContent += `[${stamp}] ${message}\n`;
  log.scrollTop = log.scrollHeight;
}

function showStep(step) {
  currentStep = step;
  for (let i = 1; i <= totalSteps; i += 1) {
    el(`step-${i}`).classList.toggle('hidden', i !== step);
  }
  el('step-indicator').textContent = `Step ${step} of ${totalSteps}`;
  el('progress-fill').style.width = `${(step / totalSteps) * 100}%`;
  el('prev-step').disabled = step === 1;
  el('next-step').disabled = step === totalSteps;
  refreshSummary();
}

function requireStepCompletion(step) {
  if (step === 1 && !state.preflight) {
    appendLog('Run Preflight first.');
    return false;
  }
  if (step === 2 && !state.workspaceRoot) {
    appendLog('Select workspace root before continuing.');
    return false;
  }
  if (step === 4 && !state.selectedModel) {
    appendLog('Select an Ollama model before continuing.');
    return false;
  }
  return true;
}

function refreshSummary() {
  const summary = {
    brainer_root: state.brainerRoot || '(not detected)',
    workspace_root: state.workspaceRoot || '(pending)',
    default_workspace: state.defaultWorkspace || '(none)',
    github_token: state.githubToken ? 'configured' : 'missing',
    github_webhook_secret: state.githubSecret ? 'configured' : 'missing',
    github_webhook_url: state.githubWebhookUrl || '(not provided)',
    allow_insecure_webhooks: state.allowInsecureWebhooks,
    ollama_model: state.selectedModel || '(pending)',
    mcp_agents: Array.from(state.selectedAgents),
    setup_applied: state.setupApplied,
  };
  el('review-summary').textContent = JSON.stringify(summary, null, 2);
}

function renderPreflight(status) {
  const rows = [
    ['Brainer Root Detected', state.brainerRoot ? 'OK' : 'FAIL'],
    ['Docker CLI', status.docker_cli ? 'OK' : 'FAIL'],
    ['Docker Compose', status.docker_compose ? 'OK' : 'FAIL'],
    ['Docker Daemon', status.docker_daemon ? 'OK' : 'FAIL'],
    ['Ollama CLI', status.ollama_cli ? 'OK' : 'FAIL'],
    ['Ollama Running', status.ollama_running ? 'OK' : 'FAIL'],
  ];

  el('preflight-results').innerHTML = rows
    .map(
      ([k, v]) => `<div class="kv"><span>${k}</span><span class="${v === 'OK' ? 'ok' : 'fail'}">${v}</span></div>`,
    )
    .join('');
}

function renderWorkspaceChildren() {
  const list = el('workspace-list');
  const select = el('default-workspace');
  list.innerHTML = '';
  select.innerHTML = '<option value="">-- ninguno --</option>';

  if (!state.workspaceChildren.length) {
    list.innerHTML = '<div class="hint">No subfolders found.</div>';
    return;
  }

  state.workspaceChildren.forEach((entry) => {
    const row = document.createElement('div');
    row.className = 'item';
    row.innerHTML = `<span><strong>${entry.name}</strong><br /><small>${entry.path}</small></span>`;
    list.appendChild(row);

    const option = document.createElement('option');
    option.value = entry.name;
    option.textContent = entry.name;
    select.appendChild(option);
  });

  if (state.defaultWorkspace) {
    select.value = state.defaultWorkspace;
  }
}

function updateModelSelector() {
  const select = el('selected-model');
  const seen = new Set();
  const options = ['<option value="">-- seleccionar --</option>'];
  state.installedModels.forEach((m) => {
    if (!seen.has(m)) {
      seen.add(m);
      options.push(`<option value="${m}">${m}</option>`);
    }
  });
  state.suggestedModels.forEach((m) => {
    if (!seen.has(m.name)) {
      seen.add(m.name);
      options.push(`<option value="${m.name}">${m.name} (recommended)</option>`);
    }
  });
  select.innerHTML = options.join('');
  if (state.selectedModel) {
    select.value = state.selectedModel;
  }
}

function renderInstalledModels() {
  const box = el('models-installed');
  if (!state.installedModels.length) {
    box.innerHTML = '<div class="hint">No Ollama models detected.</div>';
    return;
  }
  box.innerHTML = state.installedModels
    .map(
      (name) => `<div class="item"><span>${name}</span><button class="btn" data-remove-model="${name}">Delete</button></div>`,
    )
    .join('');

  box.querySelectorAll('[data-remove-model]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const modelName = btn.getAttribute('data-remove-model');
      appendLog(`Removing model ${modelName}...`);
      try {
        await invoke('delete_ollama_model', { modelName });
        appendLog(`Model removed: ${modelName}`);
        await refreshModels();
      } catch (error) {
        appendLog(`Failed to remove ${modelName}: ${error}`);
      }
    });
  });
}

function renderSuggestedModels() {
  const box = el('models-suggested');
  if (!state.suggestedModels.length) {
    box.innerHTML = '<div class="hint">No recommendations available.</div>';
    return;
  }
  box.innerHTML = state.suggestedModels
    .map(
      (m) => `<div class="item"><span><strong>${m.name}</strong><br /><small>${m.why}</small></span><button class="btn" data-install-model="${m.name}">Install</button></div>`,
    )
    .join('');

  box.querySelectorAll('[data-install-model]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const modelName = btn.getAttribute('data-install-model');
      appendLog(`Pulling Ollama model ${modelName}. This may take several minutes...`);
      btn.disabled = true;
      try {
        const output = await invoke('pull_ollama_model', { modelName });
        appendLog(`Installed model ${modelName}. Status: ${output.status}`);
        state.selectedModel = modelName;
        await refreshModels();
      } catch (error) {
        appendLog(`Failed to install ${modelName}: ${error}`);
      } finally {
        btn.disabled = false;
      }
    });
  });
}

async function refreshModels() {
  try {
    const [installed, suggested] = await Promise.all([
      invoke('get_ollama_models'),
      invoke('get_recommended_models'),
    ]);
    state.installedModels = installed || [];
    state.suggestedModels = suggested || [];
    if (!state.selectedModel && state.installedModels.length) {
      state.selectedModel = state.installedModels[0];
    }
    renderInstalledModels();
    renderSuggestedModels();
    updateModelSelector();
    refreshSummary();
  } catch (error) {
    appendLog(`Failed loading models: ${error}`);
  }
}

async function detectBrainerRoot() {
  try {
    state.brainerRoot = await invoke('detect_brainer_root');
    appendLog(`Detected Brainer root: ${state.brainerRoot}`);
  } catch (error) {
    appendLog(`Could not auto-detect Brainer root: ${error}`);
  }
}

async function runPreflight() {
  appendLog('Running preflight checks...');
  try {
    state.preflight = await invoke('run_preflight_checks');
    renderPreflight(state.preflight);
    appendLog('Preflight completed.');
  } catch (error) {
    appendLog(`Preflight failed: ${error}`);
  }
  refreshSummary();
}

async function pickWorkspaceRoot() {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title: 'Select workspace master folder',
    });
    if (!selected || Array.isArray(selected)) {
      return;
    }
    state.workspaceRoot = selected;
    el('workspace-root').value = selected;
    state.workspaceChildren = await invoke('discover_workspace_children', { rootPath: selected });
    if (!state.defaultWorkspace && state.workspaceChildren.length) {
      state.defaultWorkspace = state.workspaceChildren[0].name;
    }
    renderWorkspaceChildren();
    refreshSummary();
    appendLog(`Workspace root selected: ${selected}`);
  } catch (error) {
    appendLog(`Workspace selection failed: ${error}`);
  }
}

async function applySetup() {
  if (!state.workspaceRoot) {
    appendLog('Workspace root is required.');
    return;
  }
  if (!state.selectedModel) {
    appendLog('Select an Ollama model first.');
    return;
  }

  const config = {
    brainerRoot: state.brainerRoot || null,
    workspaceRoot: state.workspaceRoot,
    defaultWorkspace: state.defaultWorkspace || null,
    githubToken: state.githubToken || null,
    githubWebhookSecret: state.githubSecret || null,
    githubWebhookUrl: state.githubWebhookUrl || null,
    ollamaModel: state.selectedModel || null,
    allowInsecureWebhooks: state.allowInsecureWebhooks,
  };

  appendLog('Applying setup: generating .env + docker compose build/up...');
  try {
    const result = await invoke('apply_setup', { config });
    state.setupApplied = true;
    appendLog(`Setup applied. Env file: ${result.env_file}`);
    result.logs.forEach((entry) => appendLog(entry));
    appendLog('Brainer stack is up.');
  } catch (error) {
    appendLog(`Setup failed: ${error}`);
  }
  refreshSummary();
}

async function runMcpInstalls() {
  const agents = Array.from(state.selectedAgents);
  if (!agents.length) {
    appendLog('No MCP agent selected.');
    return;
  }
  appendLog(`Installing MCP integrations: ${agents.join(', ')}`);
  for (const agent of agents) {
    try {
      const result = await invoke('install_agent_mcp', { agent });
      appendLog(`${agent}: ${result.message}`);
    } catch (error) {
      appendLog(`${agent}: failed (${error})`);
    }
  }
}

function wireEvents() {
  el('run-preflight').addEventListener('click', runPreflight);
  el('pick-workspace-root').addEventListener('click', pickWorkspaceRoot);
  el('refresh-models').addEventListener('click', refreshModels);
  el('selected-model').addEventListener('change', (event) => {
    state.selectedModel = event.target.value;
    refreshSummary();
  });
  el('default-workspace').addEventListener('change', (event) => {
    state.defaultWorkspace = event.target.value;
    refreshSummary();
  });

  el('github-token').addEventListener('input', (event) => {
    state.githubToken = event.target.value.trim();
    refreshSummary();
  });
  el('github-secret').addEventListener('input', (event) => {
    state.githubSecret = event.target.value.trim();
    refreshSummary();
  });
  el('github-url').addEventListener('input', (event) => {
    state.githubWebhookUrl = event.target.value.trim();
    refreshSummary();
  });
  el('allow-insecure-webhooks').addEventListener('change', (event) => {
    state.allowInsecureWebhooks = Boolean(event.target.checked);
    refreshSummary();
  });

  document.querySelectorAll('.check-grid input[type="checkbox"]').forEach((cb) => {
    cb.addEventListener('change', (event) => {
      const agent = event.target.value;
      if (event.target.checked) state.selectedAgents.add(agent);
      else state.selectedAgents.delete(agent);
      refreshSummary();
    });
  });

  el('prev-step').addEventListener('click', () => showStep(Math.max(1, currentStep - 1)));
  el('next-step').addEventListener('click', () => {
    if (!requireStepCompletion(currentStep)) return;
    showStep(Math.min(totalSteps, currentStep + 1));
  });

  el('apply-setup').addEventListener('click', applySetup);
  el('run-mcp-installs').addEventListener('click', runMcpInstalls);
}

async function bootstrap() {
  wireEvents();
  showStep(1);
  await detectBrainerRoot();
  await refreshModels();
  refreshSummary();
  appendLog('Wizard ready.');
}

bootstrap();
