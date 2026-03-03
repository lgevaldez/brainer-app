# Brainer App (Tauri Desktop)

`brainer-app` es el cliente desktop para configurar y operar Brainer de forma guiada.

## Objetivo

1. Reemplazar la configuración manual de variables por un wizard.
2. Mantener Docker/Docker Compose como prerequisito (no instalación automática).
3. Configurar workspace maestro, webhooks GitHub, modelo Ollama y MCP para agentes.

## Requisitos

1. Docker + Docker Compose instalados y daemon activo.
2. Ollama instalado (opcional pero recomendado para LLM local).
3. Node.js 20+ y Rust toolchain (para desarrollo de la app).

## Wizard (flujo)

1. **Preflight**: valida Docker, Compose y Ollama.
2. **Workspace maestro**: selector nativo de carpeta y detección de subcarpetas.
3. **GitHub**: pide token, webhook secret y webhook URL con explicaciones de seguridad.
4. **Ollama**: detecta modelos instalados; sugiere 3 modelos compatibles y permite instalar/eliminar.
5. **MCP agents**: selecciona clientes (Codex/Cursor/Claude/Antigravity).
6. **Apply**: genera `.env`, ejecuta `docker compose build backend` + `docker compose up -d`, persiste settings y activa workspace por defecto.

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

1. El wizard escribe `.env` en la raíz de Brainer.
2. Luego persiste settings vía `PUT /api/settings` para reflejar configuración también en DB.
3. La instalación MCP se ejecuta vía `POST /api/installers/install/{agent}`.
