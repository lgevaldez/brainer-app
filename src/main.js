import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { confirm as confirmDialog, open } from '@tauri-apps/plugin-dialog';
import { buildCognitiveSummary, formatCognitiveSummary } from './lib/cognitive.js';
import { formatWorkspaceKind, resolveTargetWorkspacesForAgents as resolveAgentWorkspaces } from './lib/workspaces.js';

const totalSteps = 6;
let currentStep = 1;
let unlistenSetupProgress = null;
let unlistenAgentsIndexProgress = null;
let localProgressTimer = null;

const state = {
  brainerRoot: '',
  preflight: null,
  workspaceRoot: '',
  workspaceChildren: [],
  githubToken: '',
  githubSecret: '',
  githubWebhookUrl: '',
  allowInsecureWebhooks: false,
  installedModels: [],
  selectedModel: '',
  suggestedModels: [],
  selectedAgents: new Set(['codex']),
  setupApplied: false,
  setupInProgress: false,
  buildProgress: 0,
  buildStage: 'idle',
  buildMessage: 'No operator setup run yet.',
  lastProgressSignature: '',
  agentsSuggestion: null,
  agentsIncludeProjectMirrors: true,
  agentsOverwrite: false,
  agentsOutput: '',
  mcpInstallInProgress: false,
  mcpInstallProgress: 0,
  mcpInstallStage: 'idle',
  mcpInstallMessage: 'No MCP installation run yet.',
  agentsIndexMode: 'incremental',
  agentsIndexInProgress: false,
  agentsIndexProgress: 0,
  agentsIndexStage: 'idle',
  agentsIndexMessage: 'No index run yet.',
  lastAgentsIndexSignature: '',
  cognitiveTelemetry: null,
  cognitiveBackground: null,
};

const el = (id) => document.getElementById(id);

function appendLog(message) {
  const log = el('execution-log');
  if (!log) return;
  const stamp = new Date().toISOString();
  log.textContent += `[${stamp}] ${message}\n`;
  log.scrollTop = log.scrollHeight;
}

function setSetupButtonsDisabled(disabled) {
  const applyBtn = el('apply-setup');
  const mcpBtn = el('run-mcp-installs');
  if (applyBtn) applyBtn.disabled = disabled;
  if (mcpBtn) mcpBtn.disabled = disabled;
}

function setBuildProgress(percent, label, message, isError = false) {
  const panel = el('build-progress-panel');
  const fill = el('build-progress-fill');
  const percentEl = el('build-progress-percent');
  const labelEl = el('build-progress-label');
  const msgEl = el('build-progress-message');

  if (!panel || !fill || !percentEl || !labelEl || !msgEl) return;

  const clamped = Math.max(0, Math.min(100, Number(percent) || 0));
  state.buildProgress = clamped;
  state.buildStage = label || state.buildStage;
  state.buildMessage = message || state.buildMessage;

  panel.classList.remove('hidden');
  panel.classList.toggle('error', Boolean(isError));
  fill.style.width = `${clamped}%`;
  percentEl.textContent = `${clamped}%`;
  labelEl.textContent = label || 'Running operator setup...';
  msgEl.textContent = message || '';

  refreshSummary();
}

function setMcpInstallProgress(percent, label, message, isError = false, details = '') {
  const panel = el('mcp-install-panel');
  const fill = el('mcp-progress-fill');
  const percentEl = el('mcp-progress-percent');
  const labelEl = el('mcp-progress-label');
  const msgEl = el('mcp-progress-message');
  const detailsEl = el('mcp-progress-results');

  if (!panel || !fill || !percentEl || !labelEl || !msgEl || !detailsEl) return;

  const clamped = Math.max(0, Math.min(100, Number(percent) || 0));
  state.mcpInstallProgress = clamped;
  state.mcpInstallStage = label || state.mcpInstallStage;
  state.mcpInstallMessage = message || state.mcpInstallMessage;

  panel.classList.remove('hidden');
  panel.classList.toggle('error', Boolean(isError));
  fill.style.width = clamped + '%';
  percentEl.textContent = clamped + '%';
  labelEl.textContent = label || 'Installing Brainer MCP integrations...';
  msgEl.textContent = message || '';
  detailsEl.textContent = details || '';

  refreshSummary();
}

function setAgentsIndexProgress(percent, label, message, isError = false) {
  const panel = el('agents-index-panel');
  const fill = el('agents-index-fill');
  const percentEl = el('agents-index-percent');
  const labelEl = el('agents-index-label');
  const msgEl = el('agents-index-message');

  if (!panel || !fill || !percentEl || !labelEl || !msgEl) return;

  const clamped = Math.max(0, Math.min(100, Number(percent) || 0));
  state.agentsIndexProgress = clamped;
  state.agentsIndexStage = label || state.agentsIndexStage;
  state.agentsIndexMessage = message || state.agentsIndexMessage;

  panel.classList.remove('hidden');
  panel.classList.toggle('error', Boolean(isError));
  fill.style.width = clamped + '%';
  percentEl.textContent = clamped + '%';
  labelEl.textContent = label || 'Indexing before AGENTS generation...';
  msgEl.textContent = message || '';

  refreshSummary();
}

async function ensureSetupProgressListener() {
  if (unlistenSetupProgress) return;

  try {
    unlistenSetupProgress = await listen('setup-progress', (event) => {
      const payload = event.payload || {};
      const progress = Number(payload.progress ?? 0);
      const stage = String(payload.stage ?? 'setup');
      const message = String(payload.message ?? '');

      setBuildProgress(progress, stage, message, stage === 'failed');

      const signature = progress + ':' + stage + ':' + message;
      if (signature !== state.lastProgressSignature) {
        appendLog('Setup ' + stage + ': ' + message + ' (' + progress + '%)');
        state.lastProgressSignature = signature;
      }
    });
  } catch (error) {
    appendLog('Progress listener unavailable, using local fallback: ' + error);
  }
}

async function ensureAgentsIndexProgressListener() {
  if (unlistenAgentsIndexProgress) return;

  try {
    unlistenAgentsIndexProgress = await listen('agents-index-progress', (event) => {
      const payload = event.payload || {};
      const progress = Number(payload.progress ?? 0);
      const stage = String(payload.stage ?? 'indexing');
      const message = String(payload.message ?? '');

      setAgentsIndexProgress(progress, stage, message, stage === 'failed');

      const signature = progress + ':' + stage + ':' + message;
      if (signature !== state.lastAgentsIndexSignature) {
        appendLog('Index ' + stage + ': ' + message + ' (' + progress + '%)');
        state.lastAgentsIndexSignature = signature;
      }

      if (stage === 'complete' || stage === 'failed') {
        state.agentsIndexInProgress = false;
      }
    });
  } catch (error) {
    appendLog('Agents index listener unavailable: ' + error);
  }
}

function startLocalProgressFallback() {
  stopLocalProgressFallback();
  localProgressTimer = setInterval(() => {
    if (!state.setupInProgress) return;
    const next = Math.min(92, state.buildProgress + 1);
    if (next > state.buildProgress) {
      setBuildProgress(next, state.buildStage || 'running', state.buildMessage || 'Running setup...');
    }
  }, 1200);
}

function stopLocalProgressFallback() {
  if (!localProgressTimer) return;
  clearInterval(localProgressTimer);
  localProgressTimer = null;
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
  if (step === 1 && !state.brainerRoot) {
    appendLog('Brainer root not detected. Use Choose Brainer Folder.');
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
    github_token: state.githubToken ? 'configured' : 'missing',
    github_webhook_secret: state.githubSecret ? 'configured' : 'missing',
    github_webhook_url: state.githubWebhookUrl || '(not provided)',
    allow_insecure_webhooks: state.allowInsecureWebhooks,
    ollama_model: state.selectedModel || '(pending)',
    mcp_agents: Array.from(state.selectedAgents),
    setup_applied: state.setupApplied,
    setup_in_progress: state.setupInProgress,
    setup_progress: `${state.buildProgress}%`,
    setup_stage: state.buildStage,
    mcp_install_in_progress: state.mcpInstallInProgress,
    mcp_install_progress: `${state.mcpInstallProgress}%`,
    mcp_install_stage: state.mcpInstallStage,
    agents_index_mode: state.agentsIndexMode,
    agents_index_in_progress: state.agentsIndexInProgress,
    agents_index_progress: `${state.agentsIndexProgress}%`,
    agents_index_stage: state.agentsIndexStage,
    agents_scope: state.workspaceChildren.length ? 'all_detected_workspaces' : '(no workspace detected)',
    agents_include_project_mirrors: state.agentsIncludeProjectMirrors,
    agents_overwrite: state.agentsOverwrite,
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
  list.innerHTML = '';

  if (!state.workspaceChildren.length) {
    list.innerHTML = '<div class="hint">No subfolders found.</div>';
    return;
  }

  state.workspaceChildren.forEach((entry) => {
    const row = document.createElement('div');
    row.className = 'item';
    row.innerHTML = `<span><strong>${entry.name}</strong><br /><small>${entry.path}</small><br /><small>kind: ${formatWorkspaceKind(entry)}</small></span>`;
    list.appendChild(row);
  });
}

function resolveTargetWorkspacesForAgents() {
  return resolveAgentWorkspaces(state.workspaceChildren || []);
}

function resolveSuggestionWorkspace() {
  const workspaces = resolveTargetWorkspacesForAgents();
  if (!workspaces.length) return '';
  return workspaces[0];
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
    const [installed, suggested] = await Promise.all([invoke('get_ollama_models'), invoke('get_recommended_models')]);
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
    const input = el('brainer-root');
    if (input) input.value = state.brainerRoot;
    appendLog(`Detected Brainer root: ${state.brainerRoot}`);
  } catch (error) {
    appendLog(`Could not auto-detect Brainer root: ${error}`);
  }
  refreshSummary();
}

async function pickBrainerRoot() {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title: 'Select Brainer backend repository folder',
    });
    if (!selected || Array.isArray(selected)) {
      return;
    }
    state.brainerRoot = selected;
    const input = el('brainer-root');
    if (input) input.value = selected;
    appendLog(`Brainer root selected manually: ${selected}`);
    refreshSummary();
  } catch (error) {
    appendLog(`Brainer folder selection failed: ${error}`);
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
    state.agentsSuggestion = null;
    el('workspace-root').value = selected;
    state.workspaceChildren = await invoke('discover_workspace_children', { rootPath: selected });
    renderWorkspaceChildren();
    refreshSummary();
    appendLog(`Workspace root selected: ${selected}`);
  } catch (error) {
    appendLog(`Workspace selection failed: ${error}`);
  }
}

async function refreshCognitiveStatus() {
  const box = el('cognitive-summary');
  if (!box) return;

  try {
    const payload = await invoke('get_cognitive_status');
    state.cognitiveTelemetry = payload?.telemetry || null;
    state.cognitiveBackground = payload?.background || null;
    const summary = buildCognitiveSummary(state.cognitiveTelemetry, state.cognitiveBackground);
    box.textContent = formatCognitiveSummary(summary);
  } catch (error) {
    box.textContent = 'Unable to fetch cognitive telemetry: ' + error;
  }
}

async function applySetup() {
  if (state.setupInProgress) {
    appendLog('Setup already running...');
    return;
  }

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
    githubToken: state.githubToken || null,
    githubWebhookSecret: state.githubSecret || null,
    githubWebhookUrl: state.githubWebhookUrl || null,
    ollamaModel: state.selectedModel || null,
    allowInsecureWebhooks: state.allowInsecureWebhooks,
  };

  appendLog('Build + Up brainer clicked.');
  state.setupInProgress = true;
  state.lastProgressSignature = '';
  setSetupButtonsDisabled(true);
  setBuildProgress(1, 'prepare', 'Preparing setup execution...');
  startLocalProgressFallback();
  await new Promise((resolve) => requestAnimationFrame(resolve));
  appendLog('Applying operator setup: generating .env + docker compose build/up...');

  try {
    const result = await invoke('apply_setup_with_progress', { config });
    state.setupApplied = true;
    appendLog(`Setup applied. Env file: ${result.env_file}`);
    (result.logs || []).forEach((entry) => appendLog(entry));
    setBuildProgress(100, 'done', 'brainer is up.');
    appendLog('brainer is up.');
    await refreshAgentsSuggestion();
    await refreshCognitiveStatus();
  } catch (error) {
    setBuildProgress(Math.max(state.buildProgress, 5), 'failed', String(error), true);
    appendLog(`Setup failed: ${error}`);
  } finally {
    stopLocalProgressFallback();
    state.setupInProgress = false;
    setSetupButtonsDisabled(false);
    refreshSummary();
  }
}

async function runMcpInstalls() {
  if (state.setupInProgress) {
    appendLog('Wait for Build + Up to finish before MCP installation.');
    setMcpInstallProgress(Math.max(state.mcpInstallProgress, 1), 'blocked', 'Setup is still running. Try again when Build + Up is done.', true);
    return;
  }

  if (state.mcpInstallInProgress) {
    appendLog('MCP installation already running...');
    return;
  }

  const agents = Array.from(state.selectedAgents);
  if (!agents.length) {
    appendLog('No MCP agent selected.');
    setMcpInstallProgress(0, 'ready', 'Select at least one agent to install MCP integration.');
    return;
  }

  const button = el('run-mcp-installs');
  const lines = [];
  let failures = 0;

  state.mcpInstallInProgress = true;
  if (button) button.disabled = true;
  setMcpInstallProgress(3, 'running', 'Installing ' + agents.length + ' MCP integration(s)...');
  appendLog('Installing MCP integrations: ' + agents.join(', '));

  for (let i = 0; i < agents.length; i += 1) {
    const agent = agents[i];
    const phaseProgress = Math.max(5, Math.floor((i / agents.length) * 90));
    setMcpInstallProgress(phaseProgress, 'running', 'Installing ' + agent + ' (' + (i + 1) + '/' + agents.length + ')...', false, lines.join('\n'));

    try {
      const result = await invoke('install_agent_mcp', { agent });
      const message = result?.message ? String(result.message) : 'Installed';
      lines.push('OK ' + agent + ': ' + message);
      appendLog(agent + ': ' + message);
    } catch (error) {
      failures += 1;
      const message = String(error);
      lines.push('FAIL ' + agent + ': ' + message);
      appendLog(agent + ': failed (' + message + ')');
    }

    const completed = i + 1;
    const progress = Math.max(10, Math.min(96, Math.round((completed / agents.length) * 96)));
    setMcpInstallProgress(progress, 'running', 'Processed ' + completed + '/' + agents.length + ' integration(s).', false, lines.join('\n'));
  }

  if (failures === 0) {
    setMcpInstallProgress(100, 'done', 'MCP installation complete (' + agents.length + '/' + agents.length + ' successful).', false, lines.join('\n'));
    appendLog('MCP installation completed successfully.');
  } else {
    setMcpInstallProgress(100, 'done_with_warnings', 'MCP installation finished with ' + failures + ' failure(s).', true, lines.join('\n'));
    appendLog('MCP installation finished with ' + failures + ' failure(s).');
  }

  state.mcpInstallInProgress = false;
  if (button) button.disabled = false;
  refreshSummary();
}

function renderAgentsSuggestionText() {
  const box = el('agents-suggestion');
  if (!box) return;

  const workspaces = resolveTargetWorkspacesForAgents();
  if (!workspaces.length) {
    box.textContent = 'Select workspace root first (Step 2).';
    return;
  }

  if (!state.agentsSuggestion) {
    if (workspaces.length === 1) {
      box.textContent = "No suggestion loaded yet for workspace '" + workspaces[0] + "'.";
    } else {
      box.textContent =
        'Suggestions are workspace-specific. Refresh loads the first detected workspace (' +
        workspaces[0] +
        '). Generation still runs across all detected workspaces.';
    }
    return;
  }

  const mode = state.agentsSuggestion.suggested_mode || 'unknown';
  const repoCount = Array.isArray(state.agentsSuggestion.top_level_repos)
    ? state.agentsSuggestion.top_level_repos.length
    : 0;
  const wsLabel = state.agentsSuggestion.workspace || '(unknown)';
  box.textContent =
    "Workspace '" +
    wsLabel +
    "' | Mode: " +
    mode +
    '. ' +
    (state.agentsSuggestion.note || '') +
    ' Repos detected: ' +
    repoCount +
    '.';
}

function renderAgentsOutput() {
  const out = el('agents-output');
  if (!out) return;
  out.textContent = state.agentsOutput || '';
}

function updateAgentsIndexModeNote() {
  const note = el('agents-index-note');
  if (!note) return;

  if (state.agentsIndexMode === 'force') {
    note.textContent = 'Force mode wipes indexed vector + graph context for this workspace and rebuilds from zero.';
    return;
  }

  if (state.agentsIndexMode === 'none') {
    note.textContent = 'No index validation will run before AGENTS generation.';
    return;
  }

  note.textContent = 'Incremental mode validates and re-indexes changed files when possible.';
}

async function refreshAgentsSuggestion() {
  const workspace = resolveSuggestionWorkspace();
  if (!workspace) {
    state.agentsSuggestion = null;
    renderAgentsSuggestionText();
    return;
  }

  const workspaces = resolveTargetWorkspacesForAgents();
  if (workspaces.length > 1) {
    appendLog(
      "Refresh suggestion is workspace-specific. Using first detected workspace '" +
        workspace +
        "' out of " +
        workspaces.length +
        '.'
    );
  } else {
    appendLog("Loading AGENTS suggestion for workspace '" + workspace + "'...");
  }

  try {
    const payload = await invoke('get_agents_suggestions', {
      workspaceName: workspace,
    });
    state.agentsSuggestion = payload;
    if (Object.prototype.hasOwnProperty.call(payload, 'recommend_generate_project_mirrors')) {
      state.agentsIncludeProjectMirrors = Boolean(payload.recommend_generate_project_mirrors);
      const cb = el('agents-include-project-mirrors');
      if (cb) cb.checked = state.agentsIncludeProjectMirrors;
    }
    renderAgentsSuggestionText();
    appendLog('AGENTS suggestion loaded.');
  } catch (error) {
    state.agentsSuggestion = null;
    renderAgentsSuggestionText();
    appendLog('AGENTS suggestion failed: ' + error);
  }
  refreshSummary();
}

async function generateAgentsTemplates() {
  const workspaces = resolveTargetWorkspacesForAgents();
  if (!workspaces.length) {
    const message = 'Select a workspace root with valid project-like folders first (Step 2) before generating AGENTS templates.';
    state.agentsOutput = 'Error: ' + message;
    renderAgentsOutput();
    appendLog(message);
    renderAgentsSuggestionText();
    window.alert('Selecciona un workspace root en el paso 2 antes de generar AGENTS.md.');
    return;
  }

  const mode = state.agentsIndexMode || 'incremental';
  if (mode === 'force') {
    const confirmed = await confirmDialog(
      workspaces.length > 1
        ? 'Force re-index will wipe indexed vector/graph context for ALL selected workspaces and rebuild from zero. Continue?'
        : 'Force re-index will wipe indexed vector/graph context for this workspace and rebuild from zero. Continue?',
      {
        title: 'Confirm Force Re-index',
        kind: 'warning',
        okLabel: 'Continue',
        cancelLabel: 'Cancel',
      },
    );
    if (!confirmed) {
      appendLog('Force re-index cancelled by user.');
      return;
    }
  }

  const generateBtn = el('generate-agents-templates');
  const refreshBtn = el('refresh-agents-suggestion');

  if (generateBtn) generateBtn.disabled = true;
  if (refreshBtn) refreshBtn.disabled = true;

  try {
    if (mode !== 'none') {
      const force = mode === 'force';
      state.agentsIndexInProgress = true;
      state.lastAgentsIndexSignature = '';

      for (let i = 0; i < workspaces.length; i += 1) {
        const ws = workspaces[i];
        setAgentsIndexProgress(
          Math.max(2, Math.round((i / workspaces.length) * 100)),
          'prepare',
          force
            ? 'Preparing force re-index (' + (i + 1) + '/' + workspaces.length + ") for '" + ws + "'..."
            : 'Preparing incremental validation/re-index (' + (i + 1) + '/' + workspaces.length + ") for '" + ws + "'..."
        );

        appendLog(
          (force ? 'Running force re-index for workspace ' : 'Running incremental validation/re-index for workspace ') +
            "'" +
            ws +
            "' (" +
            (i + 1) +
            '/' +
            workspaces.length +
            ') before AGENTS generation...'
        );

        try {
          const result = await invoke('reindex_workspace_with_progress', {
            workspaceName: ws,
            force,
          });
          if (result && result.message) appendLog(String(result.message));
          await refreshCognitiveStatus();
        } catch (error) {
          state.agentsIndexInProgress = false;
          setAgentsIndexProgress(Math.max(state.agentsIndexProgress, 5), 'failed', String(error), true);
          appendLog('AGENTS pre-index failed: ' + error);
          return;
        }
      }

      state.agentsIndexInProgress = false;
    } else {
      setAgentsIndexProgress(0, 'skipped', 'Skipped by selection (no pre-index).');
      appendLog('Skipped pre-index before AGENTS generation.');
    }

    const report = [];
    appendLog("Generating master root AGENTS.md for '" + state.workspaceRoot + "'...");
    const masterRootPayload = await invoke('generate_master_root_agents_template', {
      workspaceRoot: state.workspaceRoot,
      workspaceNames: workspaces,
      overwrite: state.agentsOverwrite,
    });

    report.push(
      [
        'master_root: ' + state.workspaceRoot,
        'generated_files:',
        '- ' + (masterRootPayload.file_status || 'unknown') + ': ' + (masterRootPayload.path || '(unknown)'),
      ].join('\n'),
    );

    for (let i = 0; i < workspaces.length; i += 1) {
      const ws = workspaces[i];
      appendLog("Generating AGENTS templates for workspace '" + ws + "' (" + (i + 1) + '/' + workspaces.length + ')...');
      const payload = await invoke('generate_agents_templates', {
        workspaceName: ws,
        includeProjectMirrors: state.agentsIncludeProjectMirrors,
        overwrite: state.agentsOverwrite,
      });

      const lines = [];
      lines.push('workspace: ' + (payload.workspace || ws));
      lines.push('resolved_mode: ' + (payload.resolved_mode || 'unknown'));
      lines.push('repos: ' + ((payload.top_level_repos || []).join(', ') || '(none)'));
      lines.push('generated_files:');
      (payload.generated_files || []).forEach((item) => {
        const project = item.project_name ? ' [' + item.project_name + ']' : '';
        lines.push('- ' + item.status + ': ' + item.path + project);
      });

      report.push(lines.join('\n'));
    }

    state.agentsOutput = report.join('\n\n');
    renderAgentsOutput();
    appendLog('AGENTS templates generated for ' + workspaces.length + ' workspace(s).');
    await refreshCognitiveStatus();

    if (workspaces.length === 1) {
      await refreshAgentsSuggestion();
    } else {
      state.agentsSuggestion = null;
      renderAgentsSuggestionText();
    }
  } catch (error) {
    state.agentsOutput = 'Error: ' + error;
    renderAgentsOutput();
    appendLog('AGENTS template generation failed: ' + error);
  } finally {
    state.agentsIndexInProgress = false;
    if (generateBtn) generateBtn.disabled = false;
    if (refreshBtn) refreshBtn.disabled = false;
    refreshSummary();
  }
}

function wireEvents() {
  el('run-preflight').addEventListener('click', runPreflight);
  el('pick-brainer-root').addEventListener('click', pickBrainerRoot);
  el('pick-workspace-root').addEventListener('click', pickWorkspaceRoot);
  el('refresh-models').addEventListener('click', refreshModels);
  el('selected-model').addEventListener('change', (event) => {
    state.selectedModel = event.target.value;
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

  document.querySelectorAll('#step-5 .check-grid input[type="checkbox"]').forEach((cb) => {
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

  const refreshAgentsBtn = el('refresh-agents-suggestion');
  if (refreshAgentsBtn) refreshAgentsBtn.addEventListener('click', refreshAgentsSuggestion);

  const generateAgentsBtn = el('generate-agents-templates');
  if (generateAgentsBtn) generateAgentsBtn.addEventListener('click', generateAgentsTemplates);

  const refreshCognitiveBtn = el('refresh-cognitive-status');
  if (refreshCognitiveBtn) refreshCognitiveBtn.addEventListener('click', refreshCognitiveStatus);

  const includeMirrors = el('agents-include-project-mirrors');
  if (includeMirrors) {
    includeMirrors.addEventListener('change', (event) => {
      state.agentsIncludeProjectMirrors = Boolean(event.target.checked);
      refreshSummary();
    });
  }

  const overwriteAgents = el('agents-overwrite');
  if (overwriteAgents) {
    overwriteAgents.addEventListener('change', (event) => {
      state.agentsOverwrite = Boolean(event.target.checked);
      refreshSummary();
    });
  }

  const agentsIndexMode = el('agents-index-mode');
  if (agentsIndexMode) {
    agentsIndexMode.value = state.agentsIndexMode;
    agentsIndexMode.addEventListener('change', (event) => {
      state.agentsIndexMode = String(event.target.value || 'incremental');
      updateAgentsIndexModeNote();
      refreshSummary();
    });
  }
}

async function bootstrap() {
  await ensureSetupProgressListener();
  await ensureAgentsIndexProgressListener();
  wireEvents();
  showStep(1);
  setBuildProgress(0, 'ready', 'No operator setup run yet.');
  setMcpInstallProgress(0, 'ready', 'No MCP installation run yet.');
  setAgentsIndexProgress(0, 'ready', 'No index run yet.');
  await detectBrainerRoot();
  await refreshModels();
  renderAgentsSuggestionText();
  renderAgentsOutput();
  updateAgentsIndexModeNote();
  refreshSummary();
  await refreshCognitiveStatus();
  appendLog('Wizard ready.');
}

bootstrap();
