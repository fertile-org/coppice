# Agent connectors

Coppice runs agents through a **connector adapter** layer. Orchestration (queue, worktrees, result contract, ticket updates) is connector-agnostic; each adapter handles execution, streaming, and result parsing.

| Connector | Doc | Status |
|-----------|-----|--------|
| `mock` | [mock.md](mock.md) | Implemented — default for CI |
| `opencode` | [opencode.md](opencode.md) | Implemented — manual host testing |
| `claude-code` | [claude-code.md](claude-code.md) | Implemented — subprocess with subscription auth |
| `codex` | [codex.md](codex.md) | Implemented — subprocess with subscription auth |
| `kilo-code` | [kilo-code.md](kilo-code.md) | Implemented — subprocess (OpenCode-derived; daemon compat unverified) |
| `cursor` | [cursor.md](cursor.md) | Implemented — subprocess with host-managed auth |
| `shell` | [shell.md](shell.md) | Deferred |

OpenCode within-run **context compaction** is documented in [opencode.md § Context compaction](opencode.md#context-compaction).

## Connectors vs model providers vs models

| Layer | Example | Where configured |
|-------|---------|------------------|
| Connector | `opencode`, `mock` | `[agent.connectors.*]` in config.toml |
| Model provider | `zai-coding-plan` | `model_providers = [...]` in connector config (after host auth) |
| Model | `glm-5.1` | Per agent in UI (fetched live from connector) |

Host setup flow:

1. Enable connector in config
2. Authenticate on the host (`opencode auth login`, `claude login`, `agent login`, …)
3. Add provider IDs to `model_providers`
4. Create agents in UI — pick connector, provider, model from dropdowns

## Docker Compose (host CLIs)

The default server image ships **no** real agent CLIs (`mock` only). CI and smoke tests stay on mock.

To use a real connector with Compose, add a **local override** that mounts that CLI and its auth — do **not** bake mounts into the default `deploy/docker-compose.yml` (missing host paths become empty dirs; layouts differ per machine).

**Copy-paste snippets** (config + override YAML + verify commands) live on each connector page:

| Connector | Snippet |
|-----------|---------|
| `cursor` | [cursor.md § Docker](cursor.md#docker) |
| `claude-code` | [claude-code.md § Docker](claude-code.md#docker) |
| `codex` | [codex.md § Docker](codex.md#docker) |
| `kilo-code` | [kilo-code.md § Docker](kilo-code.md#docker) |
| `opencode` | [opencode.md § Docker](opencode.md#docker) |

Shared pattern:

1. Install + log in on the **host**.
2. Enable the connector in `deploy/config/config.toml`.
3. Save a local `deploy/docker-compose.<connector>.yml` from the connector doc.
4. `docker compose -f deploy/docker-compose.yml -f deploy/docker-compose.<connector>.yml up -d --force-recreate server`
5. `exec` the CLI inside the container to verify.

Prefer mounting at the **same absolute paths** as on the host and set `HOME` / `PATH` so symlinks and auth lookup keep working. Ensure host directories exist before the first `up`.

## Per-agent connector, model provider, and model

Server `config.toml` sets the **default connector** via `[agent] default_connector` and optional connector settings under `[agent.connectors.<id>]`.

Each **agent** can override the default connector and optionally set a `model_provider` and `model` (Agents page or `POST/PATCH /api/agents`). At run time the worker uses the assigned agent’s values, not the server default. Models are not stored in config — they are chosen per agent in the UI and fetched live from the connector.

**Health checks:** Coppice periodically evaluates whether each agent’s connector is registered and reachable, and whether its model provider is configured on the server. Health is separate from enabled/disabled status:

| Health | Meaning |
|--------|---------|
| `unknown` | Server started; check not run yet |
| `healthy` | Connector registered and liveness check passed |
| `missing_config` | Agent’s connector or model provider is not configured on this server |
| `unhealthy` | Connector registered but liveness check failed |

Runs are **blocked** when health is `missing_config` (clear API error). `unknown` and `unhealthy` are not blocked at the API layer — the worker may still fail if the connector is unavailable.

## API

```
GET  /api/connectors
GET  /api/connectors/{connector_id}/model-providers
GET  /api/connectors/{connector_id}/model-providers/{model_provider_id}/models
```

Model providers come from config. Models are fetched live (e.g. `opencode models <provider>`).

## Testing rules

- **CI / E2E:** always `default_connector = "mock"`.
- **Integration tests:** mock only; no network LLM calls.
- **Manual:** OpenCode on host `config.toml` with your own API keys via `opencode auth login`.
