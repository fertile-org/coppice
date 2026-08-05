# Cursor CLI Connector Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a dedicated `cursor` agent connector that runs Coppice jobs via the Cursor Agent CLI (`agent`) in non-interactive stream-json mode, at Claude Code parity (live console, session id, `--resume`), with host-managed auth only.

**Architecture:** Mirror the Claude Code / kilo-code subprocess pattern: `CursorProvider` spawns `{command} -p --trust --force --output-format stream-json --workspace <worktree>`, `CursorConsolePublisher` maps NDJSON to `cursor.console.*` events, `cursor_models` parses `agent models` text, and the job worker persists/resumes `session_id` like `claude-code`. No SDK, no Coppice-managed API keys, no Cursor `--worktree`.

**Tech Stack:** Rust (Axum, Tokio, serde_json), existing `AgentProvider` / `RunStreamHandle` / `extract_result_from_text`, React TicketDrawer routing, Vitest

**Spec:** [docs/superpowers/specs/2026-08-05-cursor-cli-connector-design.md](../specs/2026-08-05-cursor-cli-connector-design.md)

---

## File map

| File | Responsibility |
|------|----------------|
| `config/src/lib.rs` | `CursorConnectorConfig` under `[agent.connectors.cursor]` |
| `config.example.toml`, `deploy/config/default.toml` | Document / default disabled connector |
| `fixtures/cursor/{done,blocked,agentic,error}.jsonl` | Unit-test stream fixtures |
| `server/src/providers/cursor_console.rs` | NDJSON → `cursor.console.*` |
| `server/src/providers/cursor_models.rs` | Parse / list `agent models` |
| `server/src/providers/cursor.rs` | `AgentProvider` subprocess runner |
| `server/src/providers/mod.rs`, `registry.rs` | Module + registration |
| `server/src/api/connectors.rs` | Live models for `cursor` |
| `server/src/services/agent_health.rs` | Health for `cursor` + model provider |
| `server/src/workers/job_worker.rs` | `session_created_tx` + `load_resume_session_id` |
| `server/src/api/ws/live.rs` | Subprocess recovery list |
| `web/src/features/tickets/TicketDrawer.tsx` (+ test) | Route `cursor` → `ClaudeLiveConsole` |
| `docs/providers/cursor.md` (+ indexes, architecture blurb) | Operator docs |

---

### Task 1: Config — `CursorConnectorConfig`

**Files:**
- Modify: `config/src/lib.rs`
- Modify: `config.example.toml`
- Modify: `deploy/config/default.toml`

- [ ] **Step 1: Write failing config tests**

In `config/src/lib.rs` test module (alongside existing kilo/codex tests), add:

```rust
#[test]
fn deserializes_cursor_connector() {
    let toml = r#"
        [agent]
        default_connector = "cursor"
        worktrees_path = "./data/worktrees"
        worker_count = 2

        [agent.connectors.cursor]
        enabled = true
        command = "cursor-agent"
        run_timeout_secs = 900
        model_providers = ["cursor"]
    "#;
    #[derive(Deserialize)]
    struct Wrapper {
        agent: AgentConfig,
    }
    let wrapper: Wrapper = toml::from_str(toml).expect("parse");
    let cfg = wrapper.agent;
    assert!(cfg.connectors.cursor.enabled);
    assert_eq!(cfg.connectors.cursor.command, "cursor-agent");
    assert_eq!(cfg.connectors.cursor.run_timeout_secs, 900);
    assert_eq!(cfg.connectors.cursor.model_providers, vec!["cursor"]);
}

#[test]
fn cursor_connector_defaults() {
    let cfg = CursorConnectorConfig::default();
    assert!(!cfg.enabled);
    assert_eq!(cfg.command, "agent");
    assert_eq!(cfg.run_timeout_secs, 600);
    assert!(cfg.model_providers.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p coppice-config deserializes_cursor_connector cursor_connector_defaults`

Expected: FAIL (type / field missing).

- [ ] **Step 3: Implement config**

Add to `AgentConnectorsConfig`:

```rust
#[serde(default, rename = "cursor")]
pub cursor: CursorConnectorConfig,
```

Add struct (mirror kilo’s `command` field):

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CursorConnectorConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_cursor_command")]
    pub command: String,
    #[serde(default = "default_cursor_run_timeout_secs")]
    pub run_timeout_secs: u64,
    #[serde(default)]
    pub model_providers: Vec<String>,
}

pub type CursorProviderConfig = CursorConnectorConfig;

fn default_cursor_command() -> String {
    "agent".into()
}

fn default_cursor_run_timeout_secs() -> u64 {
    600
}

impl Default for CursorConnectorConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            command: default_cursor_command(),
            run_timeout_secs: default_cursor_run_timeout_secs(),
            model_providers: Vec::new(),
        }
    }
}
```

Add to `config.example.toml` and `deploy/config/default.toml`:

```toml
[agent.connectors.cursor]
enabled = false
command = "agent"
# run_timeout_secs = 3600
model_providers = []
# model_providers = ["cursor"]
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p coppice-config deserializes_cursor_connector cursor_connector_defaults`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add config/src/lib.rs config.example.toml deploy/config/default.toml
git commit -m "$(cat <<'EOF'
feat(config): add cursor connector settings

EOF
)"
```

---

### Task 2: Stream fixtures

**Files:**
- Create: `fixtures/cursor/done.jsonl`
- Create: `fixtures/cursor/blocked.jsonl`
- Create: `fixtures/cursor/agentic.jsonl`
- Create: `fixtures/cursor/error.jsonl`

- [ ] **Step 1: Write fixtures from observed Cursor CLI shapes**

`fixtures/cursor/done.jsonl` (one JSON object per line):

```jsonl
{"type":"system","subtype":"init","apiKeySource":"login","cwd":"/worktree","session_id":"sess_cursor_abc","model":"composer-2.5","permissionMode":"default"}
{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Run the Coppice job."}]},"session_id":"sess_cursor_abc"}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Reading .agent/context.md..."}]},"session_id":"sess_cursor_abc"}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Implementing changes..."}]},"session_id":"sess_cursor_abc"}
{"type":"result","subtype":"success","is_error":false,"duration_ms":5000,"duration_api_ms":4000,"result":"{\"status\":\"done\",\"summary\":\"Implemented the feature.\",\"changedFiles\":[\"src/main.rs\"],\"testsRun\":[\"cargo test\"],\"assignTo\":\"backend_engineer\",\"mentionAgents\":[],\"blockers\":[]}","session_id":"sess_cursor_abc","request_id":"req_1"}
```

`fixtures/cursor/blocked.jsonl`: same init/user/assistant pattern; final result text is a blocked contract:

```json
{"status":"blocked","blockerType":"missing_secret","summary":"Need API token","mentionAgents":[],"requiredCapabilities":[],"requiredSecrets":["DEPLOY_TOKEN"]}
```

with `is_error: false`, `subtype: "success"` (CLI succeeded; contract is blocked).

`fixtures/cursor/agentic.jsonl`: include a tool call pair before assistant/result:

```jsonl
{"type":"system","subtype":"init","cwd":"/worktree","session_id":"sess_cursor_tools","model":"composer-2.5","permissionMode":"default"}
{"type":"tool_call","subtype":"started","call_id":"tool_1","tool_call":{"shellToolCall":{"args":{"command":"cargo test -p coppice-server"}},"toolCallId":"tool_1"},"session_id":"sess_cursor_tools","timestamp_ms":1}
{"type":"tool_call","subtype":"completed","call_id":"tool_1","tool_call":{"shellToolCall":{"args":{"command":"cargo test -p coppice-server"},"result":{"success":{"exitCode":0,"stdout":"ok"}}},"toolCallId":"tool_1"},"session_id":"sess_cursor_tools","timestamp_ms":2}
{"type":"tool_call","subtype":"started","call_id":"tool_2","tool_call":{"editToolCall":{"args":{"path":"/worktree/src/cursor.rs","streamContent":"fn id() {}"}},"toolCallId":"tool_2"},"session_id":"sess_cursor_tools","timestamp_ms":3}
{"type":"tool_call","subtype":"completed","call_id":"tool_2","tool_call":{"editToolCall":{"args":{"path":"/worktree/src/cursor.rs"},"result":{"success":{"path":"/worktree/src/cursor.rs","linesAdded":1,"linesRemoved":0}}},"toolCallId":"tool_2"},"session_id":"sess_cursor_tools","timestamp_ms":4}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Done with tools."}]},"session_id":"sess_cursor_tools"}
{"type":"result","subtype":"success","is_error":false,"result":"{\"status\":\"done\",\"summary\":\"Ran tests and edited cursor.rs.\",\"changedFiles\":[\"src/cursor.rs\"],\"testsRun\":[\"cargo test\"],\"mentionAgents\":[],\"blockers\":[]}","session_id":"sess_cursor_tools"}
```

`fixtures/cursor/error.jsonl`:

```jsonl
{"type":"system","subtype":"init","cwd":"/worktree","session_id":"sess_cursor_err","model":"auto","permissionMode":"default"}
{"type":"result","subtype":"error","is_error":true,"result":"Authentication required","session_id":"sess_cursor_err"}
```

- [ ] **Step 2: Commit fixtures**

```bash
git add fixtures/cursor
git commit -m "$(cat <<'EOF'
test(fixtures): add Cursor Agent CLI stream-json samples

EOF
)"
```

---

### Task 3: `CursorConsolePublisher`

**Files:**
- Create: `server/src/providers/cursor_console.rs`
- Modify: `server/src/providers/mod.rs` (add `pub mod cursor_console;`)

- [ ] **Step 1: Write failing publisher tests**

Create `cursor_console.rs` with tests first (compile may need a stub `handle_stream_json`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::run_registry::RunStreamRegistry;
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/cursor")
    }

    fn collect_events(handle: &std::sync::Arc<crate::sessions::run_registry::RunStreamHandle>) -> Vec<serde_json::Value> {
        handle
            .buffered_tail()
            .iter()
            .filter_map(|msg| match msg {
                crate::sessions::LiveMessage::Event { event } => Some(event.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn publishes_done_fixture() {
        let raw = std::fs::read_to_string(fixtures_root().join("done.jsonl")).unwrap();
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();
        for line in raw.lines() {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            console.handle_stream_json(&handle, &value);
        }
        let events = collect_events(&handle);
        assert_eq!(events[0]["type"], "cursor.console.session");
        assert_eq!(events[0]["model"], "composer-2.5");
        assert!(events.iter().any(|e| e["type"] == "cursor.console.text"));
        let result = events.iter().find(|e| e["type"] == "cursor.console.result").unwrap();
        assert_eq!(result["contract"]["summary"], "Implemented the feature.");
    }

    #[test]
    fn publishes_tool_lifecycle_from_agentic_fixture() {
        let raw = std::fs::read_to_string(fixtures_root().join("agentic.jsonl")).unwrap();
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();
        for line in raw.lines() {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            console.handle_stream_json(&handle, &value);
        }
        let events = collect_events(&handle);
        let tools: Vec<_> = events.iter().filter(|e| e["type"] == "cursor.console.tool").collect();
        assert!(tools.iter().any(|t| t["status"] == "running" && t["title"].as_str().unwrap().contains("cargo test")));
        assert!(tools.iter().any(|t| t["status"] == "completed" && t["id"] == "tool_1"));
        assert!(tools.iter().any(|t| t["title"].as_str().unwrap().contains("cursor.rs")));
        assert!(events.iter().any(|e| e["type"] == "cursor.console.result"));
    }

    #[test]
    fn ignores_thinking_and_user_events() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(uuid::Uuid::new_v4());
        let mut console = CursorConsolePublisher::new();
        console.handle_stream_json(&handle, &serde_json::json!({
            "type": "thinking", "subtype": "delta", "text": "hmm", "session_id": "s"
        }));
        console.handle_stream_json(&handle, &serde_json::json!({
            "type": "user", "message": {"role": "user", "content": [{"type":"text","text":"hi"}]}, "session_id": "s"
        }));
        assert!(collect_events(&handle).is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p coppice-server --lib providers::cursor_console`

Expected: FAIL (module / behavior missing).

- [ ] **Step 3: Implement publisher**

Implement `CursorConsolePublisher` mirroring Claude’s event emission shapes but with `cursor.console.*` types:

- `system` + `subtype=init` → `{ type: "cursor.console.session", model }`
- `assistant` text blocks → text or result via `extract_result_from_text` (same as Claude)
- `tool_call` + `subtype` `started`/`completed`:
  - id from `call_id`
  - inspect `tool_call` object keys ending in `ToolCall` (or known names: `shellToolCall`, `editToolCall`, `readToolCall`, …)
  - shell: `variant: "shell"`, title = `args.command`
  - edit/read/write: `variant: "action"`, title includes path from `args.path`
  - unknown: `variant: "action"`, title = tool key name
  - started → `status: "running"`; completed → `completed` unless nested `result` looks like failure → `error`
- `result` → publish text/contract once (`contract_published` guard)
- ignore `thinking`, `user`, unknown types

Emit via `stream.publish(LiveMessage::Event { event })` like Claude.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p coppice-server --lib providers::cursor_console`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add server/src/providers/cursor_console.rs server/src/providers/mod.rs
git commit -m "$(cat <<'EOF'
feat(server): add Cursor live console event publisher

EOF
)"
```

---

### Task 4: `cursor_models` — parse `agent models`

**Files:**
- Create: `server/src/providers/cursor_models.rs`
- Modify: `server/src/providers/mod.rs`

- [ ] **Step 1: Write failing parse tests**

```rust
#[test]
fn parses_agent_models_lines() {
    let stdout = "\
auto - Auto (current, default)
composer-2.5 - Composer 2.5
gpt-5.5-high - GPT-5.5 1M High
";
    let models = parse_cursor_models_stdout(stdout);
    assert_eq!(models.len(), 3);
    assert_eq!(models[0].id, "auto");
    assert_eq!(models[0].name, "Auto (current, default)");
    assert_eq!(models[1].id, "composer-2.5");
    assert_eq!(models[2].id, "gpt-5.5-high");
}

#[test]
fn skips_headers_and_blank_lines() {
    let stdout = "Available models\n\nauto - Auto\n";
    let models = parse_cursor_models_stdout(stdout);
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "auto");
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p coppice-server --lib providers::cursor_models`

Expected: FAIL

- [ ] **Step 3: Implement**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelOption {
    pub id: String,
    pub name: String,
}

/// Parse `agent models` / `agent --list-models` human text:
/// `id - Display Name` per line.
pub fn parse_cursor_models_stdout(stdout: &str) -> Vec<ModelOption> {
    let mut models = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Available") || line.starts_with("Error") {
            continue;
        }
        let Some((id, name)) = line.split_once(" - ") else {
            continue;
        };
        let id = id.trim();
        let name = name.trim();
        if id.is_empty() || name.is_empty() || id.contains(' ') {
            continue;
        }
        models.push(ModelOption {
            id: id.to_string(),
            name: name.to_string(),
        });
    }
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    models
}

pub async fn list_cursor_models(command: &str) -> anyhow::Result<Vec<ModelOption>> {
    let output = tokio::process::Command::new(command)
        .arg("models")
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("cursor models failed: {stderr}");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let models = parse_cursor_models_stdout(&stdout);
    if models.is_empty() {
        anyhow::bail!("no cursor models available");
    }
    Ok(models)
}
```

Only the synthetic provider id `cursor` is used by the API (Task 6); listing always returns the full catalog when that provider is requested.

- [ ] **Step 4: Run tests — expect PASS**

Run: `cargo test -p coppice-server --lib providers::cursor_models`

- [ ] **Step 5: Commit**

```bash
git add server/src/providers/cursor_models.rs server/src/providers/mod.rs
git commit -m "$(cat <<'EOF'
feat(server): parse Cursor Agent CLI model list

EOF
)"
```

---

### Task 5: `CursorProvider` subprocess runner

**Files:**
- Create: `server/src/providers/cursor.rs`
- Modify: `server/src/providers/mod.rs` (`pub mod cursor;`)

- [ ] **Step 1: Write failing unit tests** (fixture-driven, no live CLI)

Mirror `claude_code.rs` tests:

- `provider_id` → `"cursor"`
- `extract_result_from_stream_json_done_fixture` / `blocked_fixture`
- `session_id_extracted_from_init_event` → `sess_cursor_abc`
- `error_result_is_rejected` — walk `error.jsonl`, assert helper `result_event_is_error` is true and no success contract is accepted
- `streaming_pipeline_publishes_console_events` using `CursorConsolePublisher`

Helper for error detection (used by `run` and tests):

```rust
fn result_event_is_error(value: &serde_json::Value) -> bool {
    if value.get("type").and_then(|v| v.as_str()) != Some("result") {
        return false;
    }
    if value.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
        return true;
    }
    matches!(value.get("subtype").and_then(|v| v.as_str()), Some("error"))
}
```

- [ ] **Step 2: Run tests — expect FAIL**

Run: `cargo test -p coppice-server --lib providers::cursor::`

- [ ] **Step 3: Implement `CursorProvider`**

Structure like `ClaudeCodeProvider`, differences:

```rust
let mut cmd = Command::new(&self.config.command);
cmd.arg("-p")
    .arg(coppice_run_prompt())
    .arg("--trust")
    .arg("--force")
    .arg("--output-format")
    .arg("stream-json")
    .arg("--workspace")
    .arg(worktree)
    .current_dir(worktree)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

if let Some(model) = &input.model {
    cmd.arg("--model").arg(model);
}
if let Some(sid) = &input.resume_session_id {
    if !sid.is_empty() {
        cmd.arg("--resume").arg(sid);
    }
}
```

Auth comment: host-managed via `agent login` only; Coppice does not inject credentials.

Stream loop: same cancel/timeout pattern as Claude; on each JSON line:

1. Capture `session_id` → `session_created_tx`
2. `console.handle_stream_json`
3. Accumulate assistant text via Claude-compatible `extract_assistant_text`
4. On `type == "result"`:
   - if `result_event_is_error`, break with error after wait (return `InvalidFixture` including result text)
   - else set `assistant_text` from `result` field and break

After wait: non-success exit → error; else `extract_result_from_text`.

Do **not** pass Cursor `-w` / `--worktree`.

- [ ] **Step 4: Run tests — expect PASS**

Run: `cargo test -p coppice-server --lib providers::cursor::`

- [ ] **Step 5: Commit**

```bash
git add server/src/providers/cursor.rs server/src/providers/mod.rs
git commit -m "$(cat <<'EOF'
feat(server): add Cursor Agent CLI provider

EOF
)"
```

---

### Task 6: Registry, connectors API, agent health

**Files:**
- Modify: `server/src/providers/registry.rs`
- Modify: `server/src/api/connectors.rs`
- Modify: `server/src/services/agent_health.rs`

- [ ] **Step 1: Write failing registry tests**

```rust
#[test]
fn registers_cursor_when_enabled() {
    let mut config = AppConfig::load_defaults().expect("config");
    config.agent.connectors.cursor.enabled = true;
    config.agent.connectors.cursor.model_providers = vec!["cursor".into()];
    let registry = ConnectorRegistry::from_config(&config, None);
    assert!(registry.has("cursor"));
    assert_eq!(registry.model_providers_for("cursor"), vec!["cursor"]);
}

#[test]
fn does_not_register_cursor_when_disabled() {
    let config = AppConfig::load_defaults().expect("config");
    let registry = ConnectorRegistry::from_config(&config, None);
    assert!(!registry.has("cursor"));
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p coppice-server --lib providers::registry::tests::registers_cursor`

- [ ] **Step 3: Wire registry**

- Import `CursorProvider`
- On `config.agent.connectors.cursor.enabled`, insert `"cursor"` → `CursorProvider::new(...)`
- Store `cursor_model_providers` from config
- Extend `model_providers_for` match arm `"cursor" => ...`

- [ ] **Step 4: Wire `list_models` in `connectors.rs`**

Add match arm:

```rust
"cursor" => {
    if model_provider_id != "cursor" {
        return Ok(Json(ModelListResponse { items: vec![] }));
    }
    let command = &state.config.agent.connectors.cursor.command;
    let models = crate::providers::cursor_models::list_cursor_models(command)
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(Json(ModelListResponse {
        items: models
            .into_iter()
            .map(|m| ModelResponse {
                id: m.id,
                name: m.name,
            })
            .collect(),
    }))
}
```

- [ ] **Step 5: Wire `agent_health.rs`**

Add a `"cursor"` arm identical to `"codex"` / `"kilo-code"` (check optional `model_provider` against registry; else Healthy). Without this arm, enabled cursor agents become `Unknown connector: cursor` → `missing_config`.

- [ ] **Step 6: Run registry + related tests**

Run:

```bash
cargo test -p coppice-server --lib providers::registry::tests::registers_cursor
cargo test -p coppice-server --lib providers::registry::tests::does_not_register_cursor
```

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add server/src/providers/registry.rs server/src/api/connectors.rs server/src/services/agent_health.rs
git commit -m "$(cat <<'EOF'
feat(server): register cursor connector and models API

EOF
)"
```

---

### Task 7: Job worker session + resume

**Files:**
- Modify: `server/src/workers/job_worker.rs`

- [ ] **Step 1: Extend `session_created_tx` gate**

Where connectors are listed for session capture (~line 557), add `|| connector_name == "cursor"`.

- [ ] **Step 2: Extend `load_resume_session_id`**

Change the connector guard from Claude-only to:

```rust
if connector_name != "claude-code" && connector_name != "cursor" {
    return None;
}
```

Update the doc comment to mention `cursor` uses `--resume <session_id>` like Claude Code.

- [ ] **Step 3: Commit**

```bash
git add server/src/workers/job_worker.rs
git commit -m "$(cat <<'EOF'
feat(server): persist and resume Cursor CLI sessions

EOF
)"
```

---

### Task 8: WS recovery + web live console routing

**Files:**
- Modify: `server/src/api/ws/live.rs`
- Modify: `web/src/features/tickets/TicketDrawer.tsx`
- Modify: `web/src/features/tickets/TicketDrawer.test.tsx`

- [ ] **Step 1: Add `cursor` to subprocess recovery match**

In `live.rs`, extend:

```rust
.is_some_and(|connector| {
    matches!(
        connector,
        "claude-code" | "codex" | "kilo-code" | "cursor"
    )
});
```

Update the nearby comment that lists claude-code / codex.

- [ ] **Step 2: Route TicketDrawer to `ClaudeLiveConsole`**

```tsx
liveRun?.connector === 'claude-code' ||
  liveRun?.connector === 'codex' ||
  liveRun?.connector === 'kilo-code' ||
  liveRun?.connector === 'cursor'
  ? ClaudeLiveConsole
  : LiveConsole;
```

(`cursor.console.*` already matches the generic `.console.` reducer.)

- [ ] **Step 3: Add Vitest case** (mirror kilo-code test)

In `TicketDrawer.test.tsx`, assert a run with `connector: 'cursor'` uses the structured live console (same assertion style as the existing kilo-code test).

- [ ] **Step 4: Run web test**

Run: `make web-test` (or targeted vitest for TicketDrawer)

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add server/src/api/ws/live.rs web/src/features/tickets/TicketDrawer.tsx web/src/features/tickets/TicketDrawer.test.tsx
git commit -m "$(cat <<'EOF'
feat: route Cursor runs through structured live console

EOF
)"
```

---

### Task 9: Provider documentation

**Files:**
- Create: `docs/providers/cursor.md`
- Modify: `docs/providers/README.md`
- Modify: `docs/providers.md`
- Modify: `docs/architecture.md` (providers line to include `cursor`)

- [ ] **Step 1: Write `docs/providers/cursor.md`**

Follow `docs/providers/claude-code.md` / `kilo-code.md` structure:

- ID `cursor`, status Implemented, stream backend subprocess stream-json
- Auth: host-managed `agent login` only — **do not** document Coppice API key setup
- Config TOML with `command = "agent"`, `model_providers = ["cursor"]`
- Capabilities table: flags `-p --trust --force --output-format stream-json --workspace`, session id, `--model`, `--resume`, cancel, timeout, live `cursor.console.*`
- How it works (numbered)
- Live streaming event map (from design)
- Session resume (worker wires like claude-code)
- Models via `agent models`
- Limitations: Docker/PATH must include CLI + login state; never use Cursor `-w`; no SDK

- [ ] **Step 2: Link from README tables and `docs/providers.md`**

- [ ] **Step 3: Commit**

```bash
git add docs/providers/cursor.md docs/providers/README.md docs/providers.md docs/architecture.md
git commit -m "$(cat <<'EOF'
docs: document Cursor CLI connector

EOF
)"
```

---

### Task 10: Final verification

- [ ] **Step 1: Targeted Rust tests**

```bash
cargo test -p coppice-config cursor
cargo test -p coppice-server --lib providers::cursor
cargo test -p coppice-server --lib providers::cursor_console
cargo test -p coppice-server --lib providers::cursor_models
cargo test -p coppice-server --lib providers::registry::tests::registers_cursor
cargo test -p coppice-server --lib providers::registry::tests::does_not_register_cursor
```

Expected: all PASS

- [ ] **Step 2: Clippy on touched crates**

```bash
cargo clippy -p coppice-config -p coppice-server -- -D warnings
```

Expected: clean

- [ ] **Step 3: Web tests for TicketDrawer**

```bash
cd web && npm test -- --run src/features/tickets/TicketDrawer.test.tsx
```

Expected: PASS

- [ ] **Step 4: Optional manual smoke (host only)**

With `agent` on PATH and logged in:

1. Enable connector in host `config.toml` with `model_providers = ["cursor"]`
2. Create agent `connector=cursor`, `modelProvider=cursor`, pick a model
3. Run a ticket; confirm live console + Done/Blocked contract
4. Confirm a second continued run passes `--resume` (check server debug logs / process args if needed)

- [ ] **Step 5: After a successful full `make test` for the delivery (if run), `make clean` per AGENTS.md**

Do **not** run full `make test` during iteration; reserve for final acceptance.

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| Dedicated `cursor` connector / no SDK | 5, 6 |
| Host-managed auth only | 5, 9 |
| `-p --trust --force --output-format stream-json --workspace` | 5 |
| No Cursor `-w` | 5, 9 |
| `CursorConsolePublisher` + tool summaries | 3 |
| Ignore thinking | 3 |
| `session_id` persist + `--resume` | 5, 7 |
| Error `result` fails run | 2, 5 |
| Synthetic model provider `cursor` + live models | 1, 4, 6 |
| Config `command` default `agent` | 1 |
| Registry / health / WS / TicketDrawer | 6, 7, 8 |
| Fixtures + unit tests, mock-only CI | 2–5, 10 |
| Operator docs | 9 |
| Acceptance criteria 1–6 | 10 + manual step |

## Out of scope (do not implement)

- Cursor SDK / cloud workers
- App-managed API keys
- Shared stream-json base with Claude Code
- `--stream-partial-output`
- MCP injection
