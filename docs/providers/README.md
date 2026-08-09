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

1. `coppice connector enable <id>` (or edit config)
2. `coppice connector install <id>` when using Docker (managed `$HOME` volume)
3. `coppice connector setup <id>` (vendor login)
4. `coppice connector doctor <id>`
5. Create agents in UI — pick connector, provider, model from dropdowns

## Docker Compose (managed connectors)

The default server image ships **no** real agent CLIs (`mock` only). CI and smoke stay on mock.

Compose mounts a **named volume** at `/home/coppice` (`HOME=/home/coppice`) for CLI binaries and auth. Compose sets `HOME=/home/coppice` and prepends `/home/coppice/.local/bin` and `/home/coppice/.opencode/bin` to `PATH` (keeps `/usr/sbin` for `gosu`). Do **not** bind-mount host `~/.local` / `~/.config` — use the operator CLI instead:

```bash
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector install cursor
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector setup cursor
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector doctor cursor

docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector install opencode
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector setup opencode
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector doctor opencode
```

Full design: [M08 — Connector operator CLI](../milestones/M08-connector-operator-cli.md). Per-connector auth notes live on each provider page under **Setup**.

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
