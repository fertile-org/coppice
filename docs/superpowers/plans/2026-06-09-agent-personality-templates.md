# Agent Personality Templates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move agent SOUL personalities from one-line DB seeds to markdown files loaded at server startup, clone into `agents.system_prompt` on create, and allow UI customization before save.

**Architecture:** Ten self-contained `server/agent_templates/{key}.md` files; `agent_templates` module loads them into `AppState.agent_templates`; migration drops `system_prompt_template` column and updates preset metadata; API handlers join DB presets with in-memory templates; `create_from_preset` clones template text (or optional body override) into `agents.system_prompt`.

**Tech Stack:** Rust (Axum, SQLx), PostgreSQL migrations, React (AgentForm), Docker (`deploy/Dockerfile.server`)

**Design spec:** `docs/superpowers/specs/2026-06-09-agent-personality-templates-design.md`

---

## File map

| File | Responsibility |
|------|----------------|
| `server/src/agent_templates/mod.rs` | Load `*.md` from disk; validate DB key coverage |
| `server/agent_templates/*.md` | Canonical SOUL personality content (10 files) |
| `server/migrations/008_agent_template_files.sql` | Drop column; UPDATE skills/responsibilities |
| `server/src/lib.rs` | `AppState.agent_templates`; disk load helper |
| `server/src/main.rs` | Load templates + validate before serving |
| `server/src/domain/agent.rs` | Remove `system_prompt_template` from `AgentPreset` |
| `server/src/services/agent_service.rs` | Preset queries without template column; `create_from_preset` takes `system_prompt` arg |
| `server/src/api/agents.rs` | Join templates for list; resolve clone on create |
| `server/tests/common/mod.rs` | Populate `agent_templates` in test `AppState` |
| `server/tests/integration_agents.rs` | Assert non-empty SOUL templates |
| `web/src/features/agents/AgentForm.tsx` | Editable prompt on create; taller textarea |
| `web/src/features/agents/AgentsPage.tsx` | Send `systemPrompt` on create |
| `deploy/Dockerfile.server` | COPY template directory into image |

---

### Task 1: Agent template loader module

**Files:**
- Create: `server/src/agent_templates/mod.rs`
- Modify: `server/src/lib.rs` (add `pub mod agent_templates;`)

- [ ] **Step 1: Write the failing unit test**

Add at bottom of `server/src/agent_templates/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn load_pm_template_from_disk() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agent_templates");
        let templates = load(&dir).expect("load templates");
        let pm = templates.get("pm").expect("pm template");
        assert!(pm.contains("# SOUL"));
        assert!(pm.contains("## Mission"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p coppice-server load_pm_template_from_disk -- --nocapture`  
Expected: FAIL — module/file missing or `pm.md` not found

- [ ] **Step 3: Implement loader**

Create `server/src/agent_templates/mod.rs`:

```rust
use std::collections::HashMap;
use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentTemplateError {
    #[error("failed to read agent templates directory: {0}")]
    Io(#[from] std::io::Error),
    #[error("agent template file name is not valid UTF-8: {0}")]
    InvalidFileName(std::path::PathBuf),
    #[error("missing agent template for preset key: {key}")]
    MissingPreset { key: String },
}

pub fn templates_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agent_templates")
}

/// Load all `*.md` files; map key = file stem (e.g. `pm.md` → `"pm"`).
pub fn load(dir: &Path) -> Result<HashMap<String, String>, AgentTemplateError> {
    let mut out = HashMap::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let key = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| AgentTemplateError::InvalidFileName(path.clone()))?
            .to_string();
        let content = std::fs::read_to_string(&path)?;
        out.insert(key, content);
    }
    Ok(out)
}

pub async fn ensure_all_presets_have_templates(
    pool: &sqlx::PgPool,
    templates: &HashMap<String, String>,
) -> Result<(), AgentTemplateError> {
    let keys: Vec<String> = sqlx::query_scalar("SELECT key FROM agent_presets ORDER BY key")
        .fetch_all(pool)
        .await
        .map_err(|e| AgentTemplateError::Io(std::io::Error::other(e)))?;

    for key in keys {
        if !templates.contains_key(&key) {
            return Err(AgentTemplateError::MissingPreset { key });
        }
    }
    Ok(())
}
```

Add to `server/src/lib.rs` after other mod declarations:

```rust
pub mod agent_templates;
```

- [ ] **Step 4: Run test to verify it still fails (no pm.md yet)**

Run: `cargo test -p coppice-server load_pm_template_from_disk -- --nocapture`  
Expected: FAIL — `pm template` not found (loader works but directory empty)

- [ ] **Step 5: Commit**

```bash
git add server/src/agent_templates/mod.rs server/src/lib.rs
git commit -m "feat(server): add agent template loader module"
```

---

### Task 2: SOUL markdown template files

**Files:**
- Create: `server/agent_templates/pm.md`
- Create: `server/agent_templates/tech_lead.md`
- Create: `server/agent_templates/frontend_engineer.md`
- Create: `server/agent_templates/backend_engineer.md`
- Create: `server/agent_templates/dba.md`
- Create: `server/agent_templates/qc.md`
- Create: `server/agent_templates/reviewer.md`
- Create: `server/agent_templates/devops.md`
- Create: `server/agent_templates/security.md`
- Create: `server/agent_templates/research.md`

- [ ] **Step 1: Create `server/agent_templates/pm.md`**

Assemble from design spec sections (SOUL intro + shared backbone + Mission + PM Operating Mode/Delegation). Full file must include, in order:

1. `# SOUL` intro (spec lines 310–316)
2. `## Stance` through `## Autonomy` (spec lines 145–194)
3. `## Mission` (spec lines 254–274)
4. `## Tone & Communication` through `## Self-Improvement` (spec lines 198–248)
5. PM `## Operating Mode` and `## Delegation Rules` (spec lines 278–298)

- [ ] **Step 2: Create the nine non-PM templates**

For each file, assemble from spec:

- `# SOUL` intro for that role (spec per-role section)
- Shared sections: Stance, Accountability, Pushback, Autonomy (**no Mission**)
- Tone, Standards, Lookup Protocol, Escalation, Self-Improvement (shared)
- Role-specific Operating Mode + Delegation Rules

Files and keys must match DB preset keys exactly.

- [ ] **Step 3: Verify all templates load**

Run:

```bash
cargo test -p coppice-server load_pm_template_from_disk -- --nocapture
test - $(ls server/agent_templates/*.md | wc -l | tr -d ' ') -eq 10
rg -l '^# SOUL' server/agent_templates/*.md | wc -l
rg -l '^## Mission' server/agent_templates/pm.md
! rg -l '^## Mission' server/agent_templates/tech_lead.md
```

Expected: unit test PASS; 10 files; only `pm.md` contains `## Mission`

- [ ] **Step 4: Commit**

```bash
git add server/agent_templates/
git commit -m "feat(server): add SOUL agent personality template files"
```

---

### Task 3: Database migration

**Files:**
- Create: `server/migrations/008_agent_template_files.sql`

- [ ] **Step 1: Write migration**

Create `server/migrations/008_agent_template_files.sql`:

```sql
ALTER TABLE agent_presets DROP COLUMN system_prompt_template;

UPDATE agent_presets SET
  skills = ARRAY['planning','requirements','prioritization','decomposition','assignment'],
  responsibilities = ARRAY['refine ticket scope','split oversized work','recommend agent assignment','escalate blockers','synthesize cross-ticket status']
WHERE key = 'pm';

UPDATE agent_presets SET
  skills = ARRAY['architecture','system design','technical review','tradeoff analysis'],
  responsibilities = ARRAY['guide implementation approach','review designs and significant changes','flag architectural risk']
WHERE key = 'tech_lead';

UPDATE agent_presets SET
  skills = ARRAY['UI implementation','component design','accessibility','frontend testing'],
  responsibilities = ARRAY['implement frontend tickets','follow project UI conventions','fix UI defects','raise frontend tech debt']
WHERE key = 'frontend_engineer';

UPDATE agent_presets SET
  skills = ARRAY['API design','services','persistence','backend testing'],
  responsibilities = ARRAY['implement backend tickets','follow project service conventions','fix backend defects','raise backend tech debt']
WHERE key = 'backend_engineer';

UPDATE agent_presets SET
  skills = ARRAY['postgres','schema design','migrations','query performance'],
  responsibilities = ARRAY['review schema changes','inspect query and migration risk','suggest index and data safety improvements']
WHERE key = 'dba';

UPDATE agent_presets SET
  skills = ARRAY['testing','QA','regression analysis','acceptance criteria'],
  responsibilities = ARRAY['verify ticket acceptance criteria','design and run test scenarios','report defects with reproduction steps']
WHERE key = 'qc';

UPDATE agent_presets SET
  skills = ARRAY['code review','diff analysis','maintainability'],
  responsibilities = ARRAY['review changes for correctness and scope','request fixes','approve when standards are met']
WHERE key = 'reviewer';

UPDATE agent_presets SET
  skills = ARRAY['CI/CD','containers','deployment','observability'],
  responsibilities = ARRAY['maintain pipelines and deploy paths','diagnose build/deploy failures','suggest operational improvements']
WHERE key = 'devops';

UPDATE agent_presets SET
  skills = ARRAY['threat modeling','dependency audit','secure coding'],
  responsibilities = ARRAY['review changes for security risk','flag vulnerabilities and unsafe patterns','recommend mitigations']
WHERE key = 'security';

UPDATE agent_presets SET
  skills = ARRAY['investigation','technical spikes','comparative analysis'],
  responsibilities = ARRAY['explore unknowns','summarize findings with sources','recommend follow-up tickets']
WHERE key = 'research';
```

- [ ] **Step 2: Apply migration locally**

Run: `make compose-up && make migrate`  
Expected: migration `008_agent_template_files` applies without error

- [ ] **Step 3: Commit**

```bash
git add server/migrations/008_agent_template_files.sql
git commit -m "feat(server): drop preset prompt column and refresh metadata"
```

---

### Task 4: AppState and server bootstrap

**Files:**
- Modify: `server/src/lib.rs`
- Modify: `server/src/main.rs`
- Modify: `server/tests/common/mod.rs`
- Modify: `server/tests/integration_auth.rs`

- [ ] **Step 1: Extend AppState**

In `server/src/lib.rs`, add field and helper:

```rust
use std::collections::HashMap;

pub struct AppState {
    // ... existing fields ...
    pub agent_templates: HashMap<String, String>,
}

impl AppState {
    pub fn load_agent_templates() -> HashMap<String, String> {
        let dir = crate::agent_templates::templates_dir();
        crate::agent_templates::load(&dir).expect("failed to load agent_templates from disk")
    }
}
```

Update `test_state()` to include:

```rust
agent_templates: AppState::load_agent_templates(),
```

- [ ] **Step 2: Wire main.rs startup**

In `server/src/main.rs`, after DB connect and before building `AppState`:

```rust
let agent_templates = coppice_server::AppState::load_agent_templates();
coppice_server::agent_templates::ensure_all_presets_have_templates(&db, &agent_templates)
    .await
    .map_err(|e| anyhow::anyhow!("agent template validation failed: {e}"))?;
```

Add `agent_templates` to the `AppState { ... }` literal in `main.rs`.

- [ ] **Step 3: Wire integration test AppState builders**

In `server/tests/common/mod.rs` `test_state_with_db()`:

```rust
agent_templates: coppice_server::AppState::load_agent_templates(),
```

In `server/tests/integration_auth.rs`, add the same field to its local `AppState` construction.

- [ ] **Step 4: Compile**

Run: `cargo build -p coppice-server`  
Expected: compile errors in `agent_service.rs` / `agents.rs` (next task) — that's OK if Task 5 follows immediately

- [ ] **Step 5: Commit**

```bash
git add server/src/lib.rs server/src/main.rs server/tests/common/mod.rs server/tests/integration_auth.rs
git commit -m "feat(server): load agent templates into AppState at startup"
```

---

### Task 5: Domain, service, and API layer

**Files:**
- Modify: `server/src/domain/agent.rs`
- Modify: `server/src/services/agent_service.rs`
- Modify: `server/src/api/agents.rs`

- [ ] **Step 1: Slim AgentPreset domain struct**

In `server/src/domain/agent.rs`, remove `system_prompt_template` from `AgentPreset`:

```rust
#[derive(Debug, Clone)]
pub struct AgentPreset {
    pub id: Uuid,
    pub key: String,
    pub role: String,
    pub skills: Vec<String>,
    pub responsibilities: Vec<String>,
}
```

- [ ] **Step 2: Update AgentService queries and create_from_preset**

In `server/src/services/agent_service.rs`:

Change `list_presets` SELECT to:

```sql
SELECT id, key, role, skills, responsibilities FROM agent_presets ORDER BY key ASC
```

Change preset fetch in `create_from_preset` similarly (drop `system_prompt_template`).

Change signature:

```rust
pub async fn create_from_preset(
    &self,
    preset_id: Uuid,
    name: &str,
    system_prompt: &str,
) -> Result<Agent, AgentError> {
```

Use `system_prompt` in `insert_agent` instead of `preset.system_prompt_template`.

Update `row_to_preset`:

```rust
fn row_to_preset(row: &sqlx::postgres::PgRow) -> AgentPreset {
    AgentPreset {
        id: row.get("id"),
        key: row.get("key"),
        role: row.get("role"),
        skills: row.get("skills"),
        responsibilities: row.get("responsibilities"),
    }
}
```

- [ ] **Step 3: Join templates in API handlers**

In `server/src/api/agents.rs`, add helper:

```rust
fn preset_to_response(preset: AgentPreset, templates: &std::collections::HashMap<String, String>) -> PresetResponse {
    let system_prompt_template = templates
        .get(&preset.key)
        .cloned()
        .unwrap_or_default();
    PresetResponse {
        id: preset.id,
        key: preset.key,
        role: preset.role,
        skills: preset.skills,
        responsibilities: preset.responsibilities,
        system_prompt_template,
    }
}
```

Update `list_presets`:

```rust
let presets = service.list_presets().await.map_err(map_error)?;
Ok(Json(PresetListResponse {
    items: presets
        .into_iter()
        .map(|p| preset_to_response(p, &state.agent_templates))
        .collect(),
}))
```

Update `create_agent` preset branch:

```rust
let agent = if let Some(preset_id) = body.preset_id {
    let preset = service.get_preset(preset_id).await.map_err(map_error)?;
    let default_prompt = state
        .agent_templates
        .get(&preset.key)
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let system_prompt = body.system_prompt.as_deref().unwrap_or(default_prompt.as_str());
    service
        .create_from_preset(preset_id, &body.name, system_prompt)
        .await
        .map_err(map_error)?
} else {
    // unchanged custom-agent branch
};
```

Add `get_preset` to `AgentService`:

```rust
pub async fn get_preset(&self, preset_id: Uuid) -> Result<AgentPreset, AgentError> {
    let row = sqlx::query(
        r#"
        SELECT id, key, role, skills, responsibilities
        FROM agent_presets
        WHERE id = $1
        "#,
    )
    .bind(preset_id)
    .fetch_optional(self.pool)
    .await?
    .ok_or(AgentError::PresetNotFound)?;
    Ok(row_to_preset(&row))
}
```

Refactor `create_from_preset` to call `get_preset` internally if preferred — either way is fine if behavior matches.

- [ ] **Step 4: Run server tests**

Run: `cargo test -p coppice-server -- --nocapture`  
Expected: PASS (integration tests may need Task 6 assertion updates)

- [ ] **Step 5: Commit**

```bash
git add server/src/domain/agent.rs server/src/services/agent_service.rs server/src/api/agents.rs
git commit -m "feat(server): serve agent templates from AppState, clone on create"
```

---

### Task 6: Integration test assertions

**Files:**
- Modify: `server/tests/integration_agents.rs`

- [ ] **Step 1: Strengthen preset list test**

In `list_presets_has_ten_entries`, after `assert_eq!(len, 10)`:

```rust
let first = &body["items"][0];
let template = first["systemPromptTemplate"].as_str().unwrap();
assert!(template.contains("# SOUL"), "expected SOUL template, got: {template}");
```

- [ ] **Step 2: Strengthen create-from-preset test**

In `create_agent_from_preset`, after create:

```rust
let template = preset["systemPromptTemplate"].as_str().unwrap();
assert_eq!(agent["systemPrompt"].as_str().unwrap(), template);
```

- [ ] **Step 3: Run integration tests**

Run: `cargo test -p coppice-server integration_agents -- --nocapture`  
Expected: PASS (requires Postgres per `DATABASE_URL` / compose)

- [ ] **Step 4: Run full workspace checks**

Run:

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add server/tests/integration_agents.rs
git commit -m "test(server): assert SOUL templates in preset and clone flows"
```

---

### Task 7: Frontend — editable prompt on create

**Files:**
- Modify: `web/src/features/agents/AgentForm.tsx`
- Modify: `web/src/features/agents/AgentsPage.tsx`

- [ ] **Step 1: Allow editing system prompt on create**

In `web/src/features/agents/AgentForm.tsx`, change system prompt textarea:

```tsx
rows={20}
// remove readOnly={mode === 'create'}
```

- [ ] **Step 2: Send systemPrompt when creating from preset**

In `web/src/features/agents/AgentsPage.tsx` `handleSubmit` (create dialog):

```tsx
await createAgent.mutateAsync({
  name: formValues.name.trim(),
  presetId: presetId || undefined,
  systemPrompt: formValues.systemPrompt,
  connector: formValues.connector,
  modelProvider: formValues.modelProvider || undefined,
  model: formValues.model || undefined,
});
```

- [ ] **Step 3: Run web tests**

Run: `make web-test`  
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add web/src/features/agents/AgentForm.tsx web/src/features/agents/AgentsPage.tsx
git commit -m "feat(web): allow customizing SOUL prompt when creating agents"
```

---

### Task 8: Docker image ships templates

**Files:**
- Modify: `deploy/Dockerfile.server`

- [ ] **Step 1: Copy templates into runtime image**

After the existing `COPY server ./server` in the builder stage, templates are already in the build context. Add to the **runtime** stage (after `RUN mkdir -p /app/server`):

```dockerfile
COPY server/agent_templates /app/server/agent_templates
```

Runtime binary resolves `CARGO_MANIFEST_DIR` as `/app/server` at compile time, so `templates_dir()` → `/app/server/agent_templates`.

- [ ] **Step 2: Verify Docker build**

Run: `docker build -f deploy/Dockerfile.server -t coppice-server:test .`  
Expected: build succeeds

- [ ] **Step 3: Commit**

```bash
git add deploy/Dockerfile.server
git commit -m "chore(deploy): ship agent template files in server image"
```

---

## Manual smoke test

- [ ] `make compose-up && make bootstrap`
- [ ] Settings → Agents → Create from PM preset → confirm long SOUL text pre-filled and editable
- [ ] Save agent → re-open → prompt unchanged in DB
- [ ] Assign ticket → run agent → inspect worktree `.agent/context.md` contains full SOUL under **System prompt**

---

## Plan self-review

| Spec requirement | Task |
|------------------|------|
| MD files at `server/agent_templates/{key}.md` | Task 2 |
| Load at startup, fail fast on missing key | Tasks 1, 4 |
| Drop `system_prompt_template` column | Task 3 |
| Clone on create | Task 5 |
| Optional override on create (UI edit) | Tasks 5, 7 |
| PM Mission only | Task 2 verification |
| Updated skills/responsibilities | Task 3 |
| Docker COPY | Task 8 |
| UI taller + editable create | Task 7 |

No TBD placeholders. Type names consistent: `agent_templates: HashMap<String, String>`, `system_prompt` / `systemPrompt` at API boundary.
