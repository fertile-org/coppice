# Per-Agent Provider/Model + Agent Health Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let each agent specify `provider` and optional `model`, route job execution through a multi-provider registry, and show separate **status** (enabled/disabled) vs **health** (unknown → healthy / missing_config / unhealthy) on the Agents page with periodic server-side checks.

**Architecture:** Rename `agents.provider_id` → `provider`, add nullable `model`. Replace single `AppState.agent_provider` with a `ProviderRegistry` (`mock` always; `opencode` when configured). Worker resolves provider + model from the assigned agent record. `AgentHealthRegistry` (in-memory `DashMap`) stores per-agent health; a background task sets all agents to `unknown` on boot, runs an immediate check, then re-checks every `health_check_interval_secs`. API merges health into agent list responses; UI shows status and health as distinct columns.

**Tech Stack:** Rust/SQLx migration, Axum, DashMap, tokio interval, React/TanStack Query, Vitest

**Spec references:** Product design §6.1 (`providerId` + `modelConfig`), `docs/providers/README.md`

---

## File map

| Path | Responsibility |
|------|----------------|
| `server/migrations/006_agent_provider_model.sql` | Rename column, add `model` |
| `server/src/domain/agent.rs` | `provider`, `model`; remove `provider_id` |
| `server/src/domain/agent_health.rs` | `AgentHealthStatus` enum |
| `server/src/services/agent_service.rs` | SQL + validation for `provider`/`model` |
| `server/src/providers/registry.rs` | Multi-provider lookup + config availability |
| `server/src/providers/mod.rs` | `AgentRunInput.model`, registry export |
| `server/src/providers/opencode.rs` | Per-run model override |
| `server/src/services/agent_health.rs` | Health evaluation + cache |
| `server/src/workers/health_worker.rs` | Periodic health checks |
| `server/src/workers/job_worker.rs` | Resolve provider from agent |
| `server/src/api/agents.rs` | API fields + `GET /api/agent-providers` |
| `server/src/lib.rs` | `AppState` registry + health cache |
| `server/src/main.rs` | Spawn health worker |
| `config/src/lib.rs` | `health_check_interval_secs` |
| `config.example.toml` | Document new setting |
| `server/tests/integration_agents.rs` | Provider/model + health tests |
| `web/src/lib/schemas/agent.ts` | `provider`, `model`, `health` |
| `web/src/features/agents/AgentForm.tsx` | Provider/model pickers |
| `web/src/features/agents/AgentsPage.tsx` | Health column |
| `web/src/features/agents/useAgents.ts` | Types + providers query |
| `docs/providers/README.md` | Note per-agent override |

---

## Health vs status

| Concept | Storage | Values | Who sets it |
|---------|---------|--------|-------------|
| **Status** | DB `agents.enabled` | `enabled` / `disabled` | User (Agents page toggle) |
| **Health** | In-memory `AgentHealthRegistry` | `unknown`, `healthy`, `missing_config`, `unhealthy` | Server periodic checker |

**Health rules:**

| Condition | Health |
|-----------|--------|
| Server just started, check not run yet | `unknown` |
| `agent.provider` not in server `ProviderRegistry` (e.g. `opencode` removed from config) | `missing_config` |
| Provider registered but liveness check fails (e.g. opencode `/doc` unreachable) | `unhealthy` |
| Provider registered and liveness check passes | `healthy` |

`mock` liveness: always passes when registered (no external process).

---

### Task 1: Migration — `provider` + `model`

**Files:**
- Create: `server/migrations/006_agent_provider_model.sql`
- Modify: `server/src/domain/agent.rs`

- [ ] **Step 1: Write migration**

```sql
ALTER TABLE agents RENAME COLUMN provider_id TO provider;

ALTER TABLE agents
  ADD COLUMN IF NOT EXISTS model TEXT NULL;
```

- [ ] **Step 2: Update domain type**

In `server/src/domain/agent.rs`:

```rust
pub struct Agent {
    // ...existing fields...
    pub provider: String,
    pub model: Option<String>,
    // remove provider_id
}
```

- [ ] **Step 3: Run migration**

```bash
make migrate
```

Expected: applies cleanly.

- [ ] **Step 4: Commit**

```bash
git add server/migrations/006_agent_provider_model.sql server/src/domain/agent.rs
git commit -m "feat(server): rename provider_id to provider and add model column"
```

---

### Task 2: Agent service + API — `provider` and `model`

**Files:**
- Modify: `server/src/services/agent_service.rs`
- Modify: `server/src/api/agents.rs`

- [ ] **Step 1: Update all SQL in `agent_service.rs`**

Replace `provider_id` with `provider` in every SELECT/INSERT/UPDATE/RETURNING. Add `model` column.

Change method signatures: `provider_id: Option<&str>` → `provider: Option<&str>`, add `model: Option<&str>`.

`create_from_preset` defaults:

```rust
let default_provider = std::env::var("AGENT_DEFAULT_PROVIDER")
    .unwrap_or_else(|_| "mock".into());
// Or read from config passed in — for service layer use param with default "mock"
self.insert_agent(..., "mock", None, ...)
```

For M05-minimal: keep preset default `provider = "mock"`, `model = None`.

`row_to_agent`:

```rust
provider: row.get("provider"),
model: row.get("model"),
```

- [ ] **Step 2: Update API types in `agents.rs`**

```rust
struct AgentResponse {
    // ...
    provider: String,
    model: Option<String>,
    health: String,           // added Task 6 — stub "unknown" until then
    health_detail: Option<String>,
    enabled: bool,
    // remove provider_id
}

struct CreateAgentBody {
    provider: Option<String>,
    model: Option<String>,
    // remove provider_id
}

struct UpdateAgentBody {
    provider: Option<String>,
    model: Option<String>,
    // remove provider_id
}
```

Until Task 6, hardcode `health: "unknown".into()`, `health_detail: None` in `agent_to_response`.

- [ ] **Step 3: Run tests**

```bash
cargo test -p coppice-server --test integration_agents
```

Update assertions: `providerId` → `provider`.

- [ ] **Step 4: Commit**

```bash
git add server/src/services/agent_service.rs server/src/api/agents.rs server/tests/integration_agents.rs
git commit -m "feat(server): expose agent provider and model on API"
```

---

### Task 3: `AgentHealthStatus` + in-memory registry

**Files:**
- Create: `server/src/domain/agent_health.rs`
- Create: `server/src/services/agent_health.rs`
- Modify: `server/src/domain/mod.rs` (if module root exists)
- Modify: `server/src/services/mod.rs`

- [ ] **Step 1: Write failing unit test**

In `server/src/services/agent_health.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_status_serializes_snake_case() {
        assert_eq!(
            health_status_to_str(AgentHealthStatus::MissingConfig),
            "missing_config"
        );
    }

    #[test]
    fn registry_starts_unknown() {
        let reg = AgentHealthRegistry::new();
        let id = Uuid::new_v4();
        reg.ensure_agent(id);
        assert_eq!(reg.get(id).status, AgentHealthStatus::Unknown);
    }
}
```

- [ ] **Step 2: Run test — expect fail**

```bash
cargo test -p coppice-server agent_health::tests -- --nocapture
```

- [ ] **Step 3: Implement**

`server/src/domain/agent_health.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentHealthStatus {
    Unknown,
    Healthy,
    MissingConfig,
    Unhealthy,
}

pub fn health_status_to_str(s: AgentHealthStatus) -> &'static str {
    match s {
        AgentHealthStatus::Unknown => "unknown",
        AgentHealthStatus::Healthy => "healthy",
        AgentHealthStatus::MissingConfig => "missing_config",
        AgentHealthStatus::Unhealthy => "unhealthy",
    }
}
```

`server/src/services/agent_health.rs`:

```rust
use dashmap::DashMap;
use uuid::Uuid;
use crate::domain::agent_health::{AgentHealthStatus, health_status_to_str};

#[derive(Debug, Clone)]
pub struct AgentHealthRecord {
    pub status: AgentHealthStatus,
    pub detail: Option<String>,
    pub checked_at: Option<time::OffsetDateTime>,
}

pub struct AgentHealthRegistry {
    inner: DashMap<Uuid, AgentHealthRecord>,
}

impl AgentHealthRegistry {
    pub fn new() -> Self {
        Self { inner: DashMap::new() }
    }

    pub fn ensure_agent(&self, agent_id: Uuid) {
        self.inner.entry(agent_id).or_insert(AgentHealthRecord {
            status: AgentHealthStatus::Unknown,
            detail: None,
            checked_at: None,
        });
    }

    pub fn set(&self, agent_id: Uuid, status: AgentHealthStatus, detail: Option<String>) {
        self.inner.insert(agent_id, AgentHealthRecord {
            status,
            detail,
            checked_at: Some(time::OffsetDateTime::now_utc()),
        });
    }

    pub fn get(&self, agent_id: Uuid) -> AgentHealthRecord {
        self.inner
            .get(&agent_id)
            .map(|e| e.clone())
            .unwrap_or(AgentHealthRecord {
                status: AgentHealthStatus::Unknown,
                detail: None,
                checked_at: None,
            })
    }
}
```

- [ ] **Step 4: Run tests — expect pass**

- [ ] **Step 5: Commit**

```bash
git add server/src/domain/agent_health.rs server/src/services/agent_health.rs server/src/services/mod.rs
git commit -m "feat(server): add in-memory agent health registry"
```

---

### Task 4: `ProviderRegistry` — multi-provider lookup

**Files:**
- Create: `server/src/providers/registry.rs`
- Modify: `server/src/providers/mod.rs`
- Modify: `server/src/lib.rs`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn lists_configured_provider_ids() {
    let config = AppConfig::load_defaults().expect("config");
    let registry = ProviderRegistry::from_config(&config, None);
    assert!(registry.has("mock"));
    assert!(!registry.has("opencode")); // serve not started in test
}
```

- [ ] **Step 2: Implement `ProviderRegistry`**

```rust
use std::collections::HashMap;
use std::sync::Arc;

pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn AgentProvider>>,
    opencode_default_model: Option<String>,
}

impl ProviderRegistry {
    pub fn from_config(
        config: &AppConfig,
        opencode_serve: Option<Arc<OpenCodeServeManager>>,
    ) -> Self {
        let mut providers: HashMap<String, Arc<dyn AgentProvider>> = HashMap::new();
        providers.insert("mock".into(), Arc::new(MockProvider::default()));

        let opencode_available = config.agent.providers.opencode.enabled
            || opencode_serve.is_some();
        if opencode_available {
            if let Some(serve) = opencode_serve {
                providers.insert(
                    "opencode".into(),
                    Arc::new(OpenCodeProvider::new(
                        serve,
                        config.agent.providers.opencode.clone(),
                    )),
                );
            }
        }

        Self {
            providers,
            opencode_default_model: config.agent.providers.opencode.model.clone(),
        }
    }

    pub fn has(&self, name: &str) -> bool {
        self.providers.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn AgentProvider>> {
        self.providers.get(name).cloned()
    }

    pub fn configured_ids(&self) -> Vec<String> {
        let mut ids: Vec<_> = self.providers.keys().cloned().collect();
        ids.sort();
        ids
    }

    pub fn default_model_for(&self, provider: &str) -> Option<String> {
        match provider {
            "opencode" => self.opencode_default_model.clone(),
            _ => None,
        }
    }
}
```

- [ ] **Step 3: Replace `AppState.agent_provider` with `provider_registry`**

```rust
pub struct AppState {
    // ...
    pub provider_registry: Arc<ProviderRegistry>,
    pub agent_health: Arc<AgentHealthRegistry>,
    // remove agent_provider: Arc<dyn AgentProvider>
}
```

Update `test_state()`, `main.rs`, `server/tests/common/mod.rs`, `integration_auth.rs`.

Keep backward compat helper if needed:

```rust
impl AppState {
    pub fn default_provider_id(&self) -> &str {
        &self.config.agent.default_provider
    }
}
```

- [ ] **Step 4: Build**

```bash
cargo build -p coppice-server
```

Fix compile errors in `job_worker.rs` temporarily by using `registry.get("mock")` — full wiring in Task 5.

- [ ] **Step 5: Commit**

```bash
git add server/src/providers/registry.rs server/src/providers/mod.rs server/src/lib.rs server/src/main.rs server/tests/common/mod.rs
git commit -m "feat(server): add multi-provider registry"
```

---

### Task 5: Per-run provider + model in worker

**Files:**
- Modify: `server/src/providers/mod.rs`
- Modify: `server/src/providers/opencode.rs`
- Modify: `server/src/workers/job_worker.rs`
- Modify: `server/src/services/artifact_service.rs` usage in worker (provider id)

- [ ] **Step 1: Extend `AgentRunInput`**

```rust
pub struct AgentRunInput {
    // ...existing...
    pub model: Option<String>,
}
```

- [ ] **Step 2: OpenCode uses per-run model**

In `opencode.rs` `run()`:

```rust
let model = input.model.as_ref().or(self.config.model.as_ref());
if let Some(model) = model {
    args.push("--model".into());
    args.push(model.clone());
}
```

- [ ] **Step 3: Worker resolves from agent**

In `execute_job`, after loading `agent`:

```rust
let provider_name = &agent.provider;
let provider = state
    .provider_registry
    .get(provider_name)
    .ok_or_else(|| anyhow::anyhow!("agent provider not configured: {provider_name}"))?;

let model = agent.model.clone().or_else(|| {
    state.provider_registry.default_model_for(provider_name)
});

let result = provider
    .run(AgentRunInput {
        // ...
        model,
        stream: Some(stream.clone()),
        cancel_rx: Some(cancel_rx),
    })
    .await;
```

Artifact meta uses `provider_name` instead of `state.agent_provider.id()`.

Update mock provider tests: add `model: None` to `AgentRunInput` literals.

- [ ] **Step 4: Run integration agent run tests**

```bash
cargo test -p coppice-server --test integration_agent_runs
```

- [ ] **Step 5: Commit**

```bash
git add server/src/providers/mod.rs server/src/providers/opencode.rs server/src/providers/mock.rs server/src/workers/job_worker.rs
git commit -m "feat(server): resolve provider and model per agent on job run"
```

---

### Task 6: Health evaluation + periodic worker + API

**Files:**
- Modify: `server/src/services/agent_health.rs`
- Create: `server/src/workers/health_worker.rs`
- Modify: `server/src/workers/mod.rs`
- Modify: `server/src/api/agents.rs`
- Modify: `server/src/lib.rs`
- Modify: `server/src/main.rs`
- Modify: `config/src/lib.rs`
- Modify: `config.example.toml`

- [ ] **Step 1: Add config**

In `config/src/lib.rs` `AgentConfig`:

```rust
#[serde(default = "default_health_check_interval")]
pub health_check_interval_secs: u32,

fn default_health_check_interval() -> u32 { 60 }
```

`config.example.toml`:

```toml
[agent]
health_check_interval_secs = 60
```

- [ ] **Step 2: Implement `evaluate_agent_health`**

In `agent_health.rs`:

```rust
pub async fn evaluate_agent_health(
    agent: &Agent,
    registry: &ProviderRegistry,
    opencode_serve: Option<&OpenCodeServeManager>,
) -> (AgentHealthStatus, Option<String>) {
    if !registry.has(&agent.provider) {
        return (
            AgentHealthStatus::MissingConfig,
            Some(format!(
                "Provider '{}' is not configured on this server",
                agent.provider
            )),
        );
    }

    match agent.provider.as_str() {
        "mock" => (AgentHealthStatus::Healthy, None),
        "opencode" => {
            let Some(serve) = opencode_serve else {
                return (
                    AgentHealthStatus::Unhealthy,
                    Some("opencode serve is not running".into()),
                );
            };
            match check_opencode_healthy(serve.base_url()).await {
                Ok(()) => (AgentHealthStatus::Healthy, None),
                Err(err) => (AgentHealthStatus::Unhealthy, Some(err.to_string())),
            }
        }
        other => (
            AgentHealthStatus::MissingConfig,
            Some(format!("Unknown provider: {other}")),
        ),
    }
}

async fn check_opencode_healthy(base_url: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let resp = client.get(format!("{base_url}/doc")).send().await?;
    if resp.status().is_success() {
        Ok(())
    } else {
        anyhow::bail!("opencode serve returned {}", resp.status());
    }
}
```

Extract shared `wait_for_healthy` logic from `opencode_serve.rs` if useful (DRY).

- [ ] **Step 3: Health worker**

`server/src/workers/health_worker.rs`:

```rust
pub fn spawn_health_worker(state: Arc<AppState>) {
    let interval_secs = state.config.agent.health_check_interval_secs.max(10);
    tokio::spawn(async move {
        // Initial pass after 2s (lets serve finish starting)
        tokio::time::sleep(Duration::from_secs(2)).await;
        run_health_pass(&state).await;

        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs as u64));
        loop {
            interval.tick().await;
            run_health_pass(&state).await;
        }
    });
}

async fn run_health_pass(state: &AppState) {
    let pool = let Some(pool) = state.db.as_ref() else { return };
    let agents = AgentService::new(pool).list_agents().await;
    let Ok(agents) = agents else { return };

    for agent in agents {
        state.agent_health.ensure_agent(agent.id);
        let (status, detail) = evaluate_agent_health(
            &agent,
            &state.provider_registry,
            state.opencode_serve.as_deref(),
        ).await;
        state.agent_health.set(agent.id, status, detail);
    }
}
```

Call `spawn_health_worker(state.clone())` in `main.rs` after DB connect.

- [ ] **Step 4: Merge health into agent API**

```rust
fn agent_to_response(agent: Agent, health: &AgentHealthRegistry) -> AgentResponse {
    let record = health.get(agent.id);
    AgentResponse {
        // ...
        health: health_status_to_str(record.status).into(),
        health_detail: record.detail,
    }
}
```

`list_agents` / `get_agent`: pass `&state.agent_health`.

- [ ] **Step 5: Add `GET /api/agent-providers`**

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderOptionResponse {
    id: String,
    default_model: Option<String>,
}

// GET /api/agent-providers
fn list_agent_providers(state: &AppState) -> Json<ProviderListResponse> {
    let items = state.provider_registry.configured_ids().into_iter().map(|id| {
        ProviderOptionResponse {
            id: id.clone(),
            default_model: state.provider_registry.default_model_for(&id),
        }
    }).collect();
    Json(ProviderListResponse { items })
}
```

- [ ] **Step 6: Integration test**

```rust
#[tokio::test]
async fn agent_with_unknown_provider_gets_missing_config_health() {
    // create agent with provider "opencode" while server only has mock registered
    // wait for health pass (or trigger evaluate directly in test)
    // GET /api/agents — assert health == "missing_config"
}
```

- [ ] **Step 7: Commit**

```bash
git add server/src/services/agent_health.rs server/src/workers/health_worker.rs server/src/api/agents.rs server/src/main.rs config/
git commit -m "feat(server): periodic agent health checks and provider list API"
```

---

### Task 7: Frontend — provider/model form + health column

**Files:**
- Modify: `web/src/lib/schemas/agent.ts`
- Modify: `web/src/features/agents/useAgents.ts`
- Modify: `web/src/features/agents/AgentForm.tsx`
- Modify: `web/src/features/agents/AgentsPage.tsx`

- [ ] **Step 1: Update schemas**

```typescript
export const createAgentSchema = z.object({
  // ...
  provider: z.string().optional(),
  model: z.string().optional(),
  // remove providerId
});

export const updateAgentSchema = z.object({
  provider: z.string().optional(),
  model: z.string().optional(),
  // remove providerId
});
```

- [ ] **Step 2: Add types + `useAgentProviders` hook**

```typescript
export interface Agent {
  provider: string;
  model?: string | null;
  health: 'unknown' | 'healthy' | 'missing_config' | 'unhealthy';
  healthDetail?: string | null;
  // remove providerId
}

export interface AgentProviderOption {
  id: string;
  defaultModel?: string | null;
}

export function useAgentProviders() {
  return useQuery({
    queryKey: ['agent-providers'],
    queryFn: async () => {
      const res = await apiFetch('/api/agent-providers');
      const data = await res.json() as { items: AgentProviderOption[] };
      return data.items;
    },
  });
}
```

- [ ] **Step 3: AgentForm — provider + model fields**

Add to create and edit modes:

```tsx
<div>
  <label htmlFor="agent-provider">Provider</label>
  <select
    id="agent-provider"
    value={values.provider}
    onChange={(e) => updateField('provider', e.target.value)}
  >
    {providerOptions.map((p) => (
      <option key={p.id} value={p.id}>{p.id}</option>
    ))}
  </select>
</div>
<div>
  <label htmlFor="agent-model">Model</label>
  <input
    id="agent-model"
    type="text"
    placeholder={selectedProvider?.defaultModel ?? 'provider/model (optional)'}
    value={values.model}
    onChange={(e) => updateField('model', e.target.value)}
  />
  <p className="text-xs text-text-muted">
    OpenCode format: provider_id/model_id (e.g. zai-coding-plan/glm-5.1). Leave empty to use server default.
  </p>
</div>
```

`AgentFormValues`: rename `providerId` → `provider`, add `model: string`.

`presetToFormValues`: `provider: 'mock'`, `model: ''`.

Pass `providerOptions` from `AgentsPage` via `useAgentProviders()`.

- [ ] **Step 4: AgentsPage — health column**

Rename current status column header to **Status** (enabled/disabled badge — unchanged).

Add **Health** column:

```tsx
function HealthBadge({ health, detail }: { health: Agent['health']; detail?: string | null }) {
  const labels = {
    unknown: { text: 'Unknown', className: 'bg-bark-100 text-bark-600' },
    healthy: { text: 'Healthy', className: 'bg-moss-100 text-moss-800' },
    missing_config: { text: 'Missing config', className: 'bg-amber-100 text-amber-900' },
    unhealthy: { text: 'Unhealthy', className: 'bg-danger-muted text-danger' },
  };
  const { text, className } = labels[health];
  return (
    <span title={detail ?? undefined} className={`inline-flex rounded-full px-2 py-0.5 text-xs ${className}`}>
      {text}
    </span>
  );
}
```

Table headers: `Name | Role | Provider | Status | Health | Updated | Actions`

Show `agent.provider` and truncated `agent.model` under provider cell.

Poll: `useAgents` refetchInterval 30_000 ms so health updates without full page reload.

- [ ] **Step 5: Run web tests**

```bash
make web-test
```

- [ ] **Step 6: Commit**

```bash
git add web/src/features/agents web/src/lib/schemas/agent.ts
git commit -m "feat(web): agent provider/model pickers and health column"
```

---

### Task 8: Run guard + docs

**Files:**
- Modify: `server/src/services/run_service.rs` or `server/src/api/tickets.rs` (run-agent handler)
- Modify: `docs/providers/README.md`
- Modify: `docs/providers/opencode.md`

- [ ] **Step 1: Block run when health is `missing_config`**

In run-agent handler, before `start_run`:

```rust
let health = state.agent_health.get(agent_id);
if health.status == AgentHealthStatus::MissingConfig {
    return Err(StatusCode::BAD_REQUEST); // message: provider not configured
}
```

Optional: allow `unhealthy` with warning (user choice) — default block `missing_config` only.

- [ ] **Step 2: Update docs**

`docs/providers/README.md` — add section on per-agent `provider` + `model` override.

`docs/providers/opencode.md` — note agent-level model overrides server default.

- [ ] **Step 3: Full verification**

```bash
make test
make clippy
make web-test
```

- [ ] **Step 4: Commit**

```bash
git add server/src/api/tickets.rs docs/providers/
git commit -m "feat(server): block agent run when provider config missing"
```

---

## Spec coverage checklist

| Requirement | Task |
|-------------|------|
| `provider` + `model` on agent (not `provider_id`) | 1, 2 |
| Per-agent execution routing | 4, 5 |
| Status = enabled/disabled only | 7 (unchanged) |
| Health separate from status | 3, 6, 7 |
| `unknown` on startup | 3, 6 |
| `missing_config` when provider absent from server config | 3, 6 |
| `healthy` when provider works | 6 |
| Periodic health checks | 6 |
| Agents UI provider/model pickers | 7 |
| Agents UI health column | 7 |
| `GET /api/agent-providers` for dropdown | 6, 7 |

---

## Manual verification

1. `config.toml`: `opencode` enabled, global model set.
2. Create two agents: one `mock`, one `opencode` with `zai-coding-plan/glm-5.1`.
3. Agents page: both show `unknown` briefly, then `healthy`.
4. Remove `[agent.providers.opencode]` / set `enabled = false`, restart server.
5. OpenCode agent shows `missing_config`; mock stays `healthy`.
6. Run agent on mock ticket — works. Run on opencode agent — blocked with clear error.
7. Re-enable opencode, health returns to `healthy`, run succeeds.

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-09-per-agent-provider-health.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks, fast iteration
2. **Inline Execution** — implement tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
