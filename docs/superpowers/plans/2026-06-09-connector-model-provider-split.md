# Connector / Model Provider / Model Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Separate execution **connectors** (opencode, mock) from upstream **model providers** (zai-coding-plan, anthropic) and **models** (glm-5.1); host configures connectors + model providers once; UI fetches models live when picking an agent model.

**Architecture:** Rename `agents.provider` → `connector`, add `model_provider`. Config moves from `[agent.providers.*]` to `[agent.connectors.*]` with a host-declared `model_providers` list (no models in config). `ConnectorRegistry` replaces `ProviderRegistry`. New `GET /api/connectors/.../models` runs `opencode models <provider>` and parses stdout. Worker assembles `{model_provider}/{model}` for OpenCode at run time. Agents UI uses three cascading selects (connector → model provider → model).

**Tech Stack:** Rust/SQLx, Axum, tokio::process, React/TanStack Query, Vitest

---

## Terminology

| Term | Example | Configured by | Stored on agent |
|------|---------|---------------|-----------------|
| **Connector** | `opencode`, `mock` | Host in `config.toml` | `connector` |
| **Model provider** | `zai-coding-plan`, `anthropic` | Host in `config.toml` (after auth) | `model_provider` |
| **Model** | `glm-5.1` | User in UI (fetched live) | `model` |

**Not live-fetched:** model providers (require API keys / OAuth on host).  
**Live-fetched:** models only (`opencode models zai-coding-plan`).

---

## File map

| Path | Responsibility |
|------|----------------|
| `config/src/lib.rs` | `connectors` config, `model_providers`, remove `model`/`variant` |
| `config.example.toml` | Updated connector config shape |
| `deploy/config/default.toml` | Same |
| `server/migrations/007_agent_connector.sql` | `provider` → `connector`, add `model_provider`, split legacy model |
| `server/src/domain/agent.rs` | `connector`, `model_provider`, `model` |
| `server/src/services/agent_service.rs` | SQL + validation |
| `server/src/api/agents.rs` | Agent API field renames |
| `server/src/api/connectors.rs` | **NEW** — connectors, model-providers, models endpoints |
| `server/src/api/mod.rs` | Mount connectors routes |
| `server/src/providers/registry.rs` | Rename → `ConnectorRegistry`, drop `default_model_for` |
| `server/src/providers/opencode_models.rs` | **NEW** — CLI parse + list models |
| `server/src/providers/opencode.rs` | Assemble full model from parts |
| `server/src/providers/mod.rs` | `AgentRunInput.model_provider` |
| `server/src/services/agent_health.rs` | Health checks connector + model_provider |
| `server/src/workers/job_worker.rs` | Pass model_provider + model to provider |
| `server/src/main.rs` | `default_connector`, `connectors.opencode` |
| `server/tests/integration_agents.rs` | Updated field names + new API tests |
| `web/src/lib/schemas/agent.ts` | `connector`, `modelProvider`, `model` |
| `web/src/features/agents/useAgents.ts` | Connector hooks + model fetch |
| `web/src/features/agents/AgentForm.tsx` | Cascading selects |
| `web/src/features/agents/AgentsPage.tsx` | Column labels |
| `docs/providers/README.md` | Terminology + config |
| `docs/providers/opencode.md` | Updated setup |

---

## Target config

```toml
[agent]
default_connector = "mock"
worktrees_path = "./data/worktrees"
worker_count = 2
health_check_interval_secs = 60

[agent.connectors.opencode]
enabled = false
command = "opencode"
serve_hostname = "127.0.0.1"
serve_port = 4096
model_providers = ["zai-coding-plan"]
```

No `model`, no `variant` in config. Host adds providers after `opencode auth login`.

**Backward compat (serde):** accept old keys via alias so existing `config.toml` keeps working until edited:

```rust
#[serde(alias = "default_provider")]
pub default_connector: String,

#[serde(alias = "providers")]
pub connectors: AgentConnectorsConfig,
```

---

## API surface

```
GET  /api/connectors
→ { "items": [{ "id": "mock" }, { "id": "opencode" }] }

GET  /api/connectors/opencode/model-providers
→ { "items": [{ "id": "zai-coding-plan" }] }   // from config

GET  /api/connectors/opencode/model-providers/zai-coding-plan/models
→ { "items": [{ "id": "glm-5.1", "name": "glm-5.1" }] }   // live CLI

DELETE /api/agent-providers   (remove old endpoint)
```

Agent API fields rename: `provider` → `connector`, add `modelProvider`.

---

### Task 1: Config — connectors + model_providers, remove model

**Files:**
- Modify: `config/src/lib.rs`
- Modify: `config.example.toml`
- Modify: `deploy/config/default.toml`
- Test: `config/src/lib.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write failing test**

Add to `config/src/lib.rs`:

```rust
#[test]
fn opencode_connector_has_model_providers_default_empty() {
    let cfg = OpenCodeConnectorConfig::default();
    assert!(cfg.model_providers.is_empty());
}

#[test]
fn deserializes_connectors_section() {
    let toml = r#"
        [agent]
        default_connector = "mock"
        worktrees_path = "./data/worktrees"
        worker_count = 2

        [agent.connectors.opencode]
        enabled = true
        model_providers = ["zai-coding-plan", "zai"]
    "#;
    let cfg: AgentConfig = toml::from_str(toml).expect("parse");
    assert_eq!(cfg.connectors.opencode.model_providers, vec!["zai-coding-plan", "zai"]);
}
```

Add `toml` dev-dependency in `config/Cargo.toml` if not present.

- [ ] **Step 2: Run test — expect fail**

```bash
cargo test -p coppice-config -- deserializes_connectors
```

Expected: FAIL (types not defined)

- [ ] **Step 3: Implement config changes**

In `config/src/lib.rs`:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentConfig {
    #[serde(alias = "default_provider")]
    pub default_connector: String,
    pub worktrees_path: String,
    pub worker_count: u32,
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval_secs: u32,
    #[serde(default, alias = "providers")]
    pub connectors: AgentConnectorsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AgentConnectorsConfig {
    #[serde(default)]
    pub opencode: OpenCodeConnectorConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenCodeConnectorConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_opencode_command")]
    pub command: String,
    #[serde(default = "default_opencode_host")]
    pub serve_hostname: String,
    #[serde(default = "default_opencode_port")]
    pub serve_port: u16,
    #[serde(default)]
    pub model_providers: Vec<String>,
}
```

Remove `model`, `variant` fields. Rename types (`AgentProvidersConfig` → `AgentConnectorsConfig`, etc.).

Update `default_values()`:

```rust
default_connector: "mock".into(),
connectors: AgentConnectorsConfig::default(),
```

Update env mapping: add `AGENT_DEFAULT_CONNECTOR` → `agent.default_connector`, keep alias for `AGENT_DEFAULT_PROVIDER`.

Add type alias for gradual compile fix (remove in Task 4):

```rust
pub type OpenCodeProviderConfig = OpenCodeConnectorConfig;
pub type AgentProvidersConfig = AgentConnectorsConfig;
```

- [ ] **Step 4: Update config files**

`config.example.toml`:

```toml
[agent]
default_connector = "mock"
# ...

[agent.connectors.opencode]
enabled = false
command = "opencode"
serve_hostname = "127.0.0.1"
serve_port = 4096
model_providers = []
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p coppice-config
```

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add config/
git commit -m "feat(config): connectors with host-declared model_providers"
```

---

### Task 2: Migration — connector + model_provider

**Files:**
- Create: `server/migrations/007_agent_connector.sql`
- Modify: `server/src/domain/agent.rs`

- [ ] **Step 1: Write migration**

```sql
ALTER TABLE agents RENAME COLUMN provider TO connector;

ALTER TABLE agents
  ADD COLUMN IF NOT EXISTS model_provider TEXT NULL;

-- Split legacy composite model values (provider/model) into separate columns
UPDATE agents
SET
  model_provider = split_part(model, '/', 1),
  model = NULLIF(split_part(model, '/', 2), '')
WHERE model IS NOT NULL AND position('/' IN model) > 0;
```

- [ ] **Step 2: Update domain**

```rust
pub struct Agent {
    // ...
    pub connector: String,
    pub model_provider: Option<String>,
    pub model: Option<String>,
    // remove provider
}
```

- [ ] **Step 3: Run migration**

```bash
make migrate
```

Expected: applies cleanly.

- [ ] **Step 4: Commit**

```bash
git add server/migrations/007_agent_connector.sql server/src/domain/agent.rs
git commit -m "feat(server): rename provider to connector and add model_provider"
```

---

### Task 3: Agent service + agents API — connector fields

**Files:**
- Modify: `server/src/services/agent_service.rs`
- Modify: `server/src/api/agents.rs`
- Modify: `server/tests/integration_agents.rs`

- [ ] **Step 1: Update agent_service.rs**

Replace all SQL `provider` → `connector`, add `model_provider` to SELECT/INSERT/UPDATE/RETURNING.

Method signatures:

```rust
pub async fn create(
    // ...
    connector: Option<&str>,
    model_provider: Option<&str>,
    model: Option<&str>,
    enabled: Option<bool>,
) -> Result<Agent, AgentError>

pub async fn update(
    // ...
    connector: Option<&str>,
    model_provider: Option<&str>,
    model: Option<&str>,
    enabled: Option<bool>,
) -> Result<Agent, AgentError>
```

`create_from_preset` defaults: `connector = "mock"`, `model_provider = None`, `model = None`.

`row_to_agent`:

```rust
connector: row.get("connector"),
model_provider: row.get("model_provider"),
model: row.get("model"),
```

Validation in `update`/`create`:

```rust
if let Some(mp) = model_provider {
    if mp.trim().is_empty() {
        return Err(AgentError::Validation("modelProvider cannot be empty".into()));
    }
}
```

- [ ] **Step 2: Update agents.rs API types**

```rust
struct AgentResponse {
    connector: String,
    model_provider: Option<String>,
    model: Option<String>,
    // remove provider
}

struct CreateAgentBody {
    connector: Option<String>,
    model_provider: Option<String>,
    model: Option<String>,
}

struct UpdateAgentBody {
    connector: Option<String>,
    model_provider: Option<String>,
    model: Option<String>,
}
```

Update `agent_to_response`, handlers to pass new fields.

Remove `GET /api/agent-providers` route and `ProviderOptionResponse` types (replaced in Task 5).

- [ ] **Step 3: Fix compile errors from domain rename**

Grep `agent.provider` and `body.provider` across `server/` and fix.

- [ ] **Step 4: Update integration test**

In `integration_agents.rs`, change test JSON/assertions:

```rust
r#"{"name":"OpenCode Bot","role":"Developer","systemPrompt":"You are a developer","connector":"opencode"}"#
// ...
assert_eq!(agent["connector"].as_str().unwrap(), "opencode");
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p coppice-server --test integration_agents
```

Expected: PASS (health test may fail until Task 6 — if so, update assertion text only)

- [ ] **Step 6: Commit**

```bash
git add server/src/services/agent_service.rs server/src/api/agents.rs server/tests/integration_agents.rs
git commit -m "feat(server): agent connector and model_provider on API"
```

---

### Task 4: ConnectorRegistry rename + model_providers lookup

**Files:**
- Modify: `server/src/providers/registry.rs`
- Modify: `server/src/providers/mod.rs`
- Modify: `server/src/lib.rs`
- Modify: `server/src/main.rs`
- Modify: `server/tests/common/mod.rs`
- Modify: `server/tests/integration_auth.rs`

- [ ] **Step 1: Write failing test**

In `registry.rs`:

```rust
#[test]
fn lists_model_providers_from_config() {
    let mut config = AppConfig::load_defaults().expect("config");
    config.agent.connectors.opencode.model_providers = vec!["zai-coding-plan".into()];
    let registry = ConnectorRegistry::from_config(&config, None);
    assert_eq!(
        registry.model_providers_for("opencode"),
        vec!["zai-coding-plan"]
    );
    assert!(registry.model_providers_for("mock").is_empty());
}
```

- [ ] **Step 2: Rename ProviderRegistry → ConnectorRegistry**

```rust
pub struct ConnectorRegistry {
    connectors: HashMap<String, Arc<dyn AgentProvider>>,
    opencode_model_providers: Vec<String>,
}

impl ConnectorRegistry {
    pub fn from_config(
        config: &AppConfig,
        opencode_serve: Option<Arc<OpenCodeServeManager>>,
    ) -> Self {
        let mut connectors: HashMap<String, Arc<dyn AgentProvider>> = HashMap::new();
        connectors.insert("mock".into(), Arc::new(MockProvider::default()));

        let opencode_available = config.agent.connectors.opencode.enabled
            || opencode_serve.is_some();
        if opencode_available {
            if let Some(serve) = opencode_serve {
                connectors.insert(
                    "opencode".into(),
                    Arc::new(OpenCodeProvider::new(
                        serve,
                        config.agent.connectors.opencode.clone(),
                    )),
                );
            }
        }

        Self {
            connectors,
            opencode_model_providers: config.agent.connectors.opencode.model_providers.clone(),
        }
    }

    pub fn has(&self, connector: &str) -> bool {
        self.connectors.contains_key(connector)
    }

    pub fn get(&self, connector: &str) -> Option<Arc<dyn AgentProvider>> {
        self.connectors.get(connector).cloned()
    }

    pub fn configured_ids(&self) -> Vec<String> {
        let mut ids: Vec<_> = self.connectors.keys().cloned().collect();
        ids.sort();
        ids
    }

    pub fn model_providers_for(&self, connector: &str) -> Vec<String> {
        match connector {
            "opencode" => self.opencode_model_providers.clone(),
            _ => vec![],
        }
    }

    pub fn has_model_provider(&self, connector: &str, model_provider: &str) -> bool {
        self.model_providers_for(connector)
            .iter()
            .any(|p| p == model_provider)
    }
}
```

Remove `default_model_for` and `opencode_default_model`.

Add `pub use registry::ConnectorRegistry` and `pub type ProviderRegistry = ConnectorRegistry` temporarily if needed for compile.

- [ ] **Step 3: Update AppState**

```rust
pub struct AppState {
    pub connector_registry: Arc<ConnectorRegistry>,
    // rename from provider_registry
}
```

Update `provider_registry_from_config` → `connector_registry_from_config`, `main.rs`, test helpers.

Update `main.rs` opencode boot condition:

```rust
if config.agent.connectors.opencode.enabled
    || config.agent.default_connector == "opencode"
```

- [ ] **Step 4: Build**

```bash
cargo build -p coppice-server
```

Fix remaining `provider_registry` / `default_provider` references.

- [ ] **Step 5: Commit**

```bash
git add server/src/providers/registry.rs server/src/providers/mod.rs server/src/lib.rs server/src/main.rs server/tests/
git commit -m "feat(server): ConnectorRegistry with config model_providers"
```

---

### Task 5: OpenCode models CLI parser + connectors API

**Files:**
- Create: `server/src/providers/opencode_models.rs`
- Create: `server/src/api/connectors.rs`
- Modify: `server/src/api/mod.rs`
- Modify: `server/src/providers/mod.rs`

- [ ] **Step 1: Write failing parser test**

`server/src/providers/opencode_models.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelOption {
    pub id: String,
    pub name: String,
}

pub fn parse_opencode_models_stdout(stdout: &str, model_provider: &str) -> Vec<ModelOption> {
    let prefix = format!("{model_provider}/");
    let mut models = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Error") {
            continue;
        }
        let id = if let Some(rest) = line.strip_prefix(&prefix) {
            rest.to_string()
        } else if let Some((_provider, model)) = line.rsplit_once('/') {
            model.to_string()
        } else {
            line.to_string()
        };
        if !id.is_empty() {
            models.push(ModelOption {
                name: id.clone(),
                id,
            });
        }
    }
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    models
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_provider_prefixed_lines() {
        let stdout = "zai-coding-plan/glm-5.1\nzai-coding-plan/glm-4.7\n";
        let models = parse_opencode_models_stdout(stdout, "zai-coding-plan");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "glm-4.7");
        assert_eq!(models[1].id, "glm-5.1");
    }
}
```

- [ ] **Step 2: Run test — expect pass after impl**

```bash
cargo test -p coppice-server parse_opencode_models
```

- [ ] **Step 3: Add async list function**

```rust
pub async fn list_opencode_models(
    command: &str,
    model_provider: &str,
) -> anyhow::Result<Vec<ModelOption>> {
    let output = tokio::process::Command::new(command)
        .args(["models", model_provider])
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("opencode models failed: {stderr}");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_opencode_models_stdout(&stdout, model_provider))
}
```

- [ ] **Step 4: Create connectors API**

`server/src/api/connectors.rs`:

```rust
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/connectors", get(list_connectors))
        .route(
            "/api/connectors/{connector_id}/model-providers",
            get(list_model_providers),
        )
        .route(
            "/api/connectors/{connector_id}/model-providers/{model_provider_id}/models",
            get(list_models),
        )
}

async fn list_connectors(State(state): State<Arc<AppState>>, AuthUser { .. }: AuthUser) -> Json<ConnectorListResponse> {
    let items = state.connector_registry.configured_ids()
        .into_iter()
        .map(|id| ConnectorResponse { id })
        .collect();
    Json(ConnectorListResponse { items })
}

async fn list_model_providers(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path(connector_id): Path<String>,
) -> Result<Json<ModelProviderListResponse>, StatusCode> {
    if !state.connector_registry.has(&connector_id) {
        return Err(StatusCode::NOT_FOUND);
    }
    let items = state.connector_registry.model_providers_for(&connector_id)
        .into_iter()
        .map(|id| ModelProviderResponse { id })
        .collect();
    Ok(Json(ModelProviderListResponse { items }))
}

async fn list_models(
    State(state): State<Arc<AppState>>,
    AuthUser { .. }: AuthUser,
    Path((connector_id, model_provider_id)): Path<(String, String)>,
) -> Result<Json<ModelListResponse>, StatusCode> {
    if !state.connector_registry.has(&connector_id) {
        return Err(StatusCode::NOT_FOUND);
    }
    if !state.connector_registry.has_model_provider(&connector_id, &model_provider_id) {
        return Err(StatusCode::NOT_FOUND);
    }
    match connector_id.as_str() {
        "opencode" => {
            let command = &state.config.agent.connectors.opencode.command;
            let models = crate::providers::opencode_models::list_opencode_models(command, &model_provider_id)
                .await
                .map_err(|_| StatusCode::BAD_GATEWAY)?;
            Ok(Json(ModelListResponse {
                items: models.into_iter().map(|m| ModelResponse { id: m.id, name: m.name }).collect(),
            }))
        }
        "mock" => Ok(Json(ModelListResponse { items: vec![] })),
        _ => Err(StatusCode::NOT_FOUND),
    }
}
```

Wire in `server/src/api/mod.rs`:

```rust
.merge(connectors::routes())
```

- [ ] **Step 5: Integration test**

Add to `integration_agents.rs`:

```rust
#[tokio::test]
async fn list_connectors_returns_mock() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await { return; }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;

    let res = app.clone().oneshot(common::json_request(
        "GET", "/api/connectors", "", &cookie, &csrf,
    )).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    let ids: Vec<_> = body["items"].as_array().unwrap()
        .iter().map(|i| i["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"mock"));
}

#[tokio::test]
async fn list_model_providers_from_config() {
    // bootstrap with test config that sets model_providers — or patch state in common helper
    // For minimal test: if test state has empty opencode providers, only test mock returns empty
    // ...
}
```

For `list_model_providers_from_config`, extend `common::bootstrap_and_login_with_state` to accept config override OR set `model_providers` in test state's config before building app. Minimal approach: unit test covers registry; integration test only checks `/api/connectors` returns mock.

- [ ] **Step 6: Run tests**

```bash
cargo test -p coppice-server --test integration_agents
cargo test -p coppice-server opencode_models
```

- [ ] **Step 7: Commit**

```bash
git add server/src/providers/opencode_models.rs server/src/api/connectors.rs server/src/api/mod.rs server/src/providers/mod.rs server/tests/integration_agents.rs
git commit -m "feat(server): connectors API and live opencode model listing"
```

---

### Task 6: Worker, OpenCode run, health, run guard

**Files:**
- Modify: `server/src/providers/mod.rs`
- Modify: `server/src/providers/opencode.rs`
- Modify: `server/src/workers/job_worker.rs`
- Modify: `server/src/services/agent_health.rs`
- Modify: `server/src/api/tickets.rs`
- Modify: `server/tests/integration_agent_runs.rs`

- [ ] **Step 1: Extend AgentRunInput**

```rust
pub struct AgentRunInput {
    // ...existing...
    pub model_provider: Option<String>,
    pub model: Option<String>,
    // remove single composite model OR keep model as assembled full string — prefer two fields
}
```

- [ ] **Step 2: OpenCode assembles full model flag**

In `opencode.rs`:

```rust
fn assemble_opencode_model(input: &AgentRunInput) -> Option<String> {
    match (&input.model_provider, &input.model) {
        (Some(provider), Some(model)) => Some(format!("{provider}/{model}")),
        (None, Some(model)) if model.contains('/') => Some(model.clone()),
        _ => None,
    }
}

// in run():
let model = assemble_opencode_model(&input);
if let Some(model) = model {
    args.push("--model".into());
    args.push(model);
}
```

Remove fallback to `self.config.model` (field removed). Remove `variant` from config usage (drop variant flag unless re-added on agent later — YAGNI, omit for now).

- [ ] **Step 3: Worker resolves from agent**

```rust
let connector_name = &agent.connector;
let connector = state.connector_registry.get(connector_name)
    .ok_or_else(|| anyhow::anyhow!("agent connector not configured: {connector_name}"))?;

let provider_result = connector.run(AgentRunInput {
    // ...
    model_provider: agent.model_provider.clone(),
    model: agent.model.clone(),
    // ...
}).await;
```

Artifact meta: `connector: connector_name.into()`.

- [ ] **Step 4: Update health evaluation**

```rust
pub async fn evaluate_agent_health(
    agent: &Agent,
    registry: &ConnectorRegistry,
    opencode_serve: Option<&OpenCodeServeManager>,
) -> (AgentHealthStatus, Option<String>) {
    if !registry.has(&agent.connector) {
        return (
            AgentHealthStatus::MissingConfig,
            Some(format!(
                "Connector '{}' is not configured on this server",
                agent.connector
            )),
        );
    }

    match agent.connector.as_str() {
        "mock" => (AgentHealthStatus::Healthy, None),
        "opencode" => {
            if let Some(ref mp) = agent.model_provider {
                if !registry.has_model_provider("opencode", mp) {
                    return (
                        AgentHealthStatus::MissingConfig,
                        Some(format!(
                            "Model provider '{}' is not configured on this server",
                            mp
                        )),
                    );
                }
            }
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
            Some(format!("Unknown connector: {other}")),
        ),
    }
}
```

Update `health_worker.rs` to pass `connector_registry`.

- [ ] **Step 5: Update run guard in tickets.rs**

```rust
if health.status == AgentHealthStatus::MissingConfig {
    return Err((
        StatusCode::BAD_REQUEST,
        Json(json!({ "message": health.detail.unwrap_or_else(|| "Agent connector is not configured".into()) })),
    ));
}
```

Update integration test `reject_run_when_agent_provider_missing_config` to use `connector` field.

- [ ] **Step 6: Run tests**

```bash
cargo test -p coppice-server --test integration_agent_runs
cargo test -p coppice-server --test integration_agents
```

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add server/src/providers/ server/src/workers/job_worker.rs server/src/services/agent_health.rs server/src/api/tickets.rs server/tests/integration_agent_runs.rs
git commit -m "feat(server): connector-based run assembly and health checks"
```

---

### Task 7: Frontend — cascading connector / model provider / model selects

**Files:**
- Modify: `web/src/lib/schemas/agent.ts`
- Modify: `web/src/features/agents/useAgents.ts`
- Modify: `web/src/features/agents/AgentForm.tsx`
- Modify: `web/src/features/agents/AgentsPage.tsx`

- [ ] **Step 1: Update schemas**

```typescript
export const createAgentSchema = z.object({
  // ...
  connector: z.string().optional(),
  modelProvider: z.string().optional(),
  model: z.string().optional(),
});

export const updateAgentSchema = z.object({
  connector: z.string().optional(),
  modelProvider: z.string().optional(),
  model: z.string().optional(),
});
```

- [ ] **Step 2: Update useAgents.ts**

```typescript
export interface Agent {
  connector: string;
  modelProvider?: string | null;
  model?: string | null;
  // remove provider
}

export interface ConnectorOption {
  id: string;
}

export interface ModelProviderOption {
  id: string;
}

export interface ModelOption {
  id: string;
  name: string;
}

export const CONNECTORS_QUERY_KEY = ['connectors'] as const;

export function useConnectors() {
  return useQuery({
    queryKey: CONNECTORS_QUERY_KEY,
    queryFn: async () => {
      const res = await apiFetch('/api/connectors');
      const data = await res.json() as { items: ConnectorOption[] };
      return data.items;
    },
  });
}

export function useModelProviders(connectorId: string | undefined) {
  return useQuery({
    queryKey: ['model-providers', connectorId],
    enabled: !!connectorId && connectorId !== 'mock',
    queryFn: async () => {
      const res = await apiFetch(`/api/connectors/${connectorId}/model-providers`);
      const data = await res.json() as { items: ModelProviderOption[] };
      return data.items;
    },
  });
}

export function useModels(connectorId: string | undefined, modelProviderId: string | undefined) {
  return useQuery({
    queryKey: ['models', connectorId, modelProviderId],
    enabled: !!connectorId && !!modelProviderId && connectorId !== 'mock',
    queryFn: async () => {
      const res = await apiFetch(
        `/api/connectors/${connectorId}/model-providers/${modelProviderId}/models`,
      );
      const data = await res.json() as { items: ModelOption[] };
      return data.items;
    },
  });
}
```

Remove `useAgentProviders` and `AGENT_PROVIDERS_QUERY_KEY`.

- [ ] **Step 3: Update AgentForm.tsx**

```typescript
export interface AgentFormValues {
  // ...
  connector: string;
  modelProvider: string;
  model: string;
}

// Replace provider/model text fields with three selects:
// 1. Connector (from useConnectors)
// 2. Model provider (from useModelProviders(values.connector)) — hidden when connector === 'mock'
// 3. Model (from useModels(values.connector, values.modelProvider)) — hidden when mock

// On connector change: reset modelProvider and model
// On modelProvider change: reset model
```

Remove free-text model input and helper text about `provider/model` format.

- [ ] **Step 4: Update AgentsPage.tsx**

- Replace `useAgentProviders()` with `useConnectors()`
- Pass connector options to form
- Table Provider column → **Connector** showing `agent.connector`
- Sub-line: `{modelProvider}/{model}` when set

```tsx
<td className="px-4 py-3 font-body text-sm text-text-secondary">
  <div>{agent.connector}</div>
  {agent.modelProvider && agent.model && (
    <div className="mt-0.5 font-mono text-xs text-text-muted">
      {agent.modelProvider}/{agent.model}
    </div>
  )}
</td>
```

Create/edit submit bodies:

```typescript
connector: formValues.connector,
modelProvider: formValues.modelProvider || undefined,
model: formValues.model || undefined,
```

- [ ] **Step 5: Run web tests**

```bash
make web-test
```

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add web/src/features/agents web/src/lib/schemas/agent.ts
git commit -m "feat(web): cascading connector, model provider, and model selects"
```

---

### Task 8: Docs + full verification

**Files:**
- Modify: `docs/providers/README.md`
- Modify: `docs/providers/opencode.md`
- Modify: `config.toml` (user's local — document in plan only, do not commit secrets)

- [ ] **Step 1: Update docs/providers/README.md**

Add section:

```markdown
## Connectors vs model providers vs models

| Layer | Example | Where configured |
|-------|---------|------------------|
| Connector | `opencode`, `mock` | `[agent.connectors.*]` in config.toml |
| Model provider | `zai-coding-plan` | `model_providers = [...]` in connector config (after host auth) |
| Model | `glm-5.1` | Per agent in UI (fetched live from connector) |

Host setup flow:
1. Enable connector in config
2. Authenticate (`opencode auth login`)
3. Add provider IDs to `model_providers`
4. Create agents in UI — pick connector, provider, model from dropdowns
```

- [ ] **Step 2: Update docs/providers/opencode.md**

Remove server-level `model` config. Document:

```markdown
[agent.connectors.opencode]
enabled = true
model_providers = ["zai-coding-plan"]

# After opencode auth login — list provider IDs with: opencode auth list
# Models are chosen per agent in the UI (fetched via opencode models <provider>)
```

- [ ] **Step 3: Full verification**

```bash
make migrate
cargo test -p coppice-server
make web-test
make clippy
```

- [ ] **Step 4: Commit**

```bash
git add docs/providers/
git commit -m "docs: connector vs model provider terminology and config"
```

---

## Spec coverage checklist

| Requirement | Task |
|-------------|------|
| Connector vs model provider vs model terminology | 1, 8 |
| Config: connectors, no models | 1 |
| Model providers host-configured (not live) | 1, 4, 5 |
| Models fetched live for UI | 5, 7 |
| DB: connector + model_provider + model | 2, 3 |
| Remove `/api/agent-providers` | 3, 5 |
| Worker assembles `provider/model` for OpenCode | 6 |
| Health: missing connector or model_provider | 6 |
| Cascading UI selects | 7 |
| Backward compat config aliases | 1 |
| Split legacy composite model in migration | 2 |

---

## Manual verification

1. Update local `config.toml`:

```toml
[agent]
default_connector = "mock"

[agent.connectors.opencode]
enabled = true
model_providers = ["zai-coding-plan"]
```

2. `opencode auth login` on host.
3. Restart server — `GET /api/connectors/opencode/model-providers` returns `zai-coding-plan`.
4. `GET .../zai-coding-plan/models` returns live model list.
5. Create agent: connector=opencode, model provider=zai-coding-plan, model=glm-5.1.
6. Agents page shows connector + model path; health → healthy.
7. Remove `zai-coding-plan` from config, restart — agent health → `missing_config`.
8. Mock agent: no model provider/model fields shown; runs still work.

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-09-connector-model-provider-split.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks, fast iteration
2. **Inline Execution** — execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
