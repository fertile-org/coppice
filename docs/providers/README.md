# Agent providers

Coppice runs agents through a **provider adapter** layer. Orchestration (queue, worktrees, result contract, ticket updates) is provider-agnostic; each adapter handles execution, streaming, and result parsing.

| Provider | Doc | Status |
|----------|-----|--------|
| `mock` | [mock.md](mock.md) | Implemented — default for CI |
| `opencode` | [opencode.md](opencode.md) | Implemented — manual host testing |
| `claude-code` | [claude-code.md](claude-code.md) | Deferred |
| `codex` | [codex.md](codex.md) | Deferred |
| `shell` | [shell.md](shell.md) | Deferred |

## Per-agent provider and model

Server `config.toml` sets the **default** provider via `[agent] default_provider` and optional global model defaults under `[agent.providers.<id>]`.

Each **agent** can override those defaults with its own `provider` and optional `model` (Agents page or `POST/PATCH /api/agents`). At run time the worker uses the assigned agent’s values, not the server default.

**Health checks:** Coppice periodically evaluates whether each agent’s provider is registered and reachable. Health is separate from enabled/disabled status:

| Health | Meaning |
|--------|---------|
| `unknown` | Server started; check not run yet |
| `healthy` | Provider registered and liveness check passed |
| `missing_config` | Agent’s provider is not configured on this server |
| `unhealthy` | Provider registered but liveness check failed |

Runs are **blocked** when health is `missing_config` (clear API error). `unknown` and `unhealthy` are not blocked at the API layer — the worker may still fail if the provider is unavailable.

## Testing rules

- **CI / E2E:** always `default_provider = "mock"`.
- **Integration tests:** mock only; no network LLM calls.
- **Manual:** OpenCode on host `config.toml` with your own API keys via `opencode auth login`.
