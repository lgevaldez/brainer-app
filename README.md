# Brainer Operator (`brainer-app`)

`brainer-app` es el cliente desktop oficial para instalar, configurar y operar
`brainer`.

## Objetivo

1. Operar `brainer` sin depender de configuración manual dispersa.
2. Mantener Docker y Docker Compose como prerequisito, sin instalación automática.
3. Centralizar setup de workspace maestro, webhooks GitHub, Ollama y MCP.
4. Generar bootstrap operativo (`.env`, `AGENTS.md`, indexación y estado inicial).

## Requisitos

1. Docker + Docker Compose instalados y daemon activo.
2. Ollama instalado (opcional pero recomendado para LLM local).
3. Node.js 20+ y Rust toolchain (para desarrollo de la app).

## Wizard (flujo)

1. **Preflight**: valida Docker, Compose y Ollama.
2. **Workspace maestro**: selector nativo de carpeta y detección de workspaces/repos.
3. **GitHub**: pide token, webhook secret y webhook URL con explicaciones de seguridad.
4. **Ollama**: detecta modelos instalados; sugiere modelos compatibles y permite instalar/eliminar.
5. **MCP agents**: selecciona clientes (Codex/Cursor/Claude/Antigravity).
6. **Apply**: genera `.env`, levanta `brainer`, persiste settings y deja listo el bootstrap de `AGENTS.md`.

## Comandos de desarrollo

```bash
cd brainer-app
npm install
npm run tauri:dev
```

Build release:

```bash
npm run tauri:build
```

## Notas de implementación

1. El wizard escribe `.env` en la raíz de `brainer`.
2. Luego persiste settings vía `PUT /api/settings` para reflejar configuración también en DB.
3. La instalación MCP se ejecuta vía `POST /api/installers/install/{agent}`.
4. La generación de `AGENTS.md` cubre root maestro y repos detectados para sesiones de stack completo.
