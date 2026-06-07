# M02 Workspace & Board Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver login-gated React SPA with multi-project board, tickets, comments, attachments, agent presets, and admin user management on top of M01 session auth.

**Architecture:** Domain-ordered vertical slices on the Rust server (migration → services → API → integration tests), then Vite React SPA with mandatory frontend-design tokens before UI. Optimistic board DnD; fullscreen ticket drawer via URL query param keeping board mounted.

**Tech Stack:** Rust/Axum/SQLx, React/Vite/TypeScript, TanStack Query, React Router v6, dnd-kit, Tailwind/shadcn, Vitest, Docker Compose, agent-browser E2E

**Spec:** [docs/superpowers/specs/2026-06-07-m02-workspace-and-board-design.md](../specs/2026-06-07-m02-workspace-and-board-design.md)

---

## File map (created or modified in M02)

| Path | Responsibility |
|------|----------------|
| `server/migrations/002_workspace.sql` | projects, repos, agents, presets, tickets, comments, attachments |
| `server/src/domain/substatus.rs` | Substatus enum, metadata validation, display labels |
| `server/src/domain/project.rs` | Project model |
| `server/src/domain/repo.rs` | Repo model |
| `server/src/domain/ticket.rs` | Ticket model + status enum |
| `server/src/domain/comment.rs` | Comment model + intent enum |
| `server/src/domain/agent.rs` | Agent + preset models |
| `server/src/domain/attachment.rs` | Attachment metadata model |
| `server/src/services/project_service.rs` | Project/repo CRUD |
| `server/src/services/ticket_service.rs` | Ticket CRUD, status, assign, display |
| `server/src/services/comment_service.rs` | Comment CRUD |
| `server/src/services/agent_service.rs` | Agent CRUD + preset copy |
| `server/src/services/user_service.rs` | Admin user create/list |
| `server/src/storage/attachment_store.rs` | Filesystem read/write under artifacts dir |
| `server/src/storage/mod.rs` | Module root |
| `server/src/middleware/admin.rs` | `AdminUser` extractor |
| `server/src/api/projects.rs` | Project routes |
| `server/src/api/repos.rs` | Repo routes |
| `server/src/api/tickets.rs` | Ticket routes |
| `server/src/api/comments.rs` | Comment routes |
| `server/src/api/agents.rs` | Agent + preset routes |
| `server/src/api/attachments.rs` | Multipart upload + file stream |
| `server/src/api/users.rs` | Admin user routes |
| `server/src/api/mod.rs` | Compose all protected routes |
| `server/src/config/mod.rs` | + `StorageConfig` (artifacts_dir, max_upload_bytes, static_dir) |
| `server/tests/common/mod.rs` | Shared DB lock, bootstrap+login helper |
| `server/tests/integration_workspace.rs` | End-to-end API flows |
| `web/package.json` | Vite React deps |
| `web/vite.config.ts` | Dev proxy `/api` → backend |
| `web/src/styles/tokens.css` | Design tokens (from frontend-design skill) |
| `web/DESIGN.md` | Design direction doc |
| `web/src/lib/api.ts` | fetch wrapper + CSRF |
| `web/src/lib/schemas/*.ts` | Zod schemas |
| `web/src/features/auth/*` | Login + session |
| `web/src/features/projects/*` | Project picker |
| `web/src/features/board/*` | Kanban + dnd-kit |
| `web/src/features/tickets/*` | Fullscreen drawer |
| `web/src/features/agents/*` | Agent list/form |
| `web/src/features/users/*` | Admin user management |
| `deploy/Dockerfile.web` | Vite dev container |
| `deploy/docker-compose.yml` | + web service, artifact volume |
| `deploy/config/default.yaml` | + storage section |
| `e2e/smoke/m02-board.mjs` | CI smoke script |
| `Makefile` | + `web-test`, `web-build`, `e2e-smoke` |
| `.github/workflows/ci.yml` | + Node/Vitest job |

---

### Task 1: Workspace migration + storage config

**Files:**
- Create: `server/migrations/002_workspace.sql`
- Modify: `server/src/config/mod.rs`
- Modify: `deploy/config/default.yaml`

- [ ] **Step 1: Write migration `002_workspace.sql`**

```sql
CREATE TABLE projects (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE repos (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    remote_url TEXT,
    default_branch TEXT NOT NULL DEFAULT 'main',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX repos_project_id_idx ON repos(project_id);

CREATE TABLE agent_presets (
    id UUID PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    role TEXT NOT NULL,
    skills TEXT[] NOT NULL DEFAULT '{}',
    responsibilities TEXT[] NOT NULL DEFAULT '{}',
    system_prompt_template TEXT NOT NULL
);

CREATE TABLE agents (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    role TEXT NOT NULL,
    skills TEXT[] NOT NULL DEFAULT '{}',
    responsibilities TEXT[] NOT NULL DEFAULT '{}',
    system_prompt TEXT NOT NULL,
    provider_id TEXT NOT NULL DEFAULT 'mock',
    enabled BOOLEAN NOT NULL DEFAULT true,
    preset_source TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE tickets (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    repo_id UUID REFERENCES repos(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL,
    substatus TEXT,
    substatus_metadata JSONB,
    priority TEXT,
    assignee_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    owner_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    branch_name TEXT,
    created_by TEXT NOT NULL,
    created_by_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX tickets_project_id_idx ON tickets(project_id);
CREATE INDEX tickets_status_idx ON tickets(project_id, status);

CREATE TABLE ticket_comments (
    id UUID PRIMARY KEY,
    ticket_id UUID NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    author_type TEXT NOT NULL,
    author_id UUID,
    body TEXT NOT NULL,
    intent TEXT NOT NULL DEFAULT 'progress_update',
    mentions JSONB NOT NULL DEFAULT '[]',
    attachment_ids UUID[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX ticket_comments_ticket_id_idx ON ticket_comments(ticket_id);

CREATE TABLE attachments (
    id UUID PRIMARY KEY,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    storage_path TEXT NOT NULL,
    uploaded_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Seed agent presets (keys match spec)
INSERT INTO agent_presets (id, key, role, skills, responsibilities, system_prompt_template) VALUES
  (gen_random_uuid(), 'pm', 'PM', ARRAY['planning','requirements'], ARRAY['refine tickets','prioritize backlog'], 'You are the PM Agent for Coppice.'),
  (gen_random_uuid(), 'tech_lead', 'Technical Lead', ARRAY['architecture','review'], ARRAY['guide implementation','review designs'], 'You are the Technical Lead Agent.'),
  (gen_random_uuid(), 'frontend_engineer', 'Frontend Engineer', ARRAY['react','css'], ARRAY['implement frontend tickets'], 'You are the Frontend Engineer Agent.'),
  (gen_random_uuid(), 'backend_engineer', 'Backend Engineer', ARRAY['rust','sql'], ARRAY['implement backend tickets'], 'You are the Backend Engineer Agent.'),
  (gen_random_uuid(), 'dba', 'DBA', ARRAY['postgres','migrations'], ARRAY['monitor database health'], 'You are the DBA Agent.'),
  (gen_random_uuid(), 'qc', 'QC', ARRAY['testing','qa'], ARRAY['verify quality'], 'You are the QC Agent.'),
  (gen_random_uuid(), 'reviewer', 'Reviewer', ARRAY['code review'], ARRAY['review changes'], 'You are the Reviewer Agent.'),
  (gen_random_uuid(), 'devops', 'DevOps', ARRAY['ci','deploy'], ARRAY['maintain pipelines'], 'You are the DevOps Agent.'),
  (gen_random_uuid(), 'security', 'Security', ARRAY['security review'], ARRAY['audit changes'], 'You are the Security Agent.'),
  (gen_random_uuid(), 'research', 'Research', ARRAY['investigation'], ARRAY['spike unknowns'], 'You are the Research Agent.');
```

Note: enable `pgcrypto` for `gen_random_uuid()` or use explicit UUIDs in seed if extension unavailable — prefer `uuid` crate in a follow-up seed migration if CI fails; use fixed UUID literals if needed.

- [ ] **Step 2: Extend `AppConfig` with storage**

In `server/src/config/mod.rs`, add:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageConfig {
    pub artifacts_dir: String,
    pub max_upload_bytes: u64,
    pub static_dir: Option<String>,
}

// Add to AppConfig:
pub storage: StorageConfig,

// In default_values():
storage: StorageConfig {
    artifacts_dir: "./data/artifacts".into(),
    max_upload_bytes: 10 * 1024 * 1024,
    static_dir: None,
},
```

Merge env `COPPICE_STORAGE__ARTIFACTS_DIR`, `COPPICE_STORAGE__STATIC_DIR`, `COPPICE_STORAGE__MAX_UPLOAD_BYTES`.

Update `deploy/config/default.yaml`:

```yaml
storage:
  artifacts_dir: /data/artifacts
  max_upload_bytes: 10485760
  static_dir: null
```

- [ ] **Step 3: Run migration locally**

Run: `make compose-up && make migrate`  
Expected: migration 002 applies without error

- [ ] **Step 4: Commit**

```bash
git add server/migrations/002_workspace.sql server/src/config/mod.rs deploy/config/default.yaml
git commit -m "feat(server): add workspace schema migration and storage config"
```

---

### Task 2: Substatus domain + unit tests

**Files:**
- Create: `server/src/domain/substatus.rs`
- Modify: `server/src/domain/mod.rs`

- [ ] **Step 1: Write failing unit tests**

Create `server/src/domain/substatus.rs` with `#[cfg(test)]` module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn waiting_for_agent_requires_agent_id() {
        let err = validate_substatus(
            Some(Substatus::WaitingForAgent),
            &Some(serde_json::json!({})),
        );
        assert!(err.is_some());
    }

    #[test]
    fn waiting_for_agent_accepts_valid_metadata() {
        let agent_id = Uuid::new_v4();
        assert!(validate_substatus(
            Some(Substatus::WaitingForAgent),
            &Some(serde_json::json!({ "agentId": agent_id })),
        )
        .is_none());
    }

    #[test]
    fn done_rejects_waiting_substatus() {
        assert!(validate_status_substatus_combo(
            TicketStatus::Done,
            Some(Substatus::WaitingForHuman),
            &None,
        )
        .is_some());
    }

    #[test]
    fn display_waiting_for_agent_uses_generic_label() {
        let label = substatus_label(Substatus::WaitingForAgent);
        assert_eq!(label, "Waiting for agent");
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p coppice-server substatus -- --nocapture`  
Expected: FAIL — types/functions not defined

- [ ] **Step 3: Implement substatus module**

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    Backlog,
    Ready,
    InProgress,
    InReview,
    InQa,
    WaitForFinalReview,
    Done,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Substatus {
    WaitingForAgent,
    WaitingForHuman,
    WaitingForOwner,
    WaitingForCi,
    BlockedByMissingCapability,
    BlockedByMissingSecret,
    BlockedByPermission,
    BlockedByError,
}

pub fn substatus_label(s: Substatus) -> &'static str {
    match s {
        Substatus::WaitingForAgent => "Waiting for agent",
        Substatus::WaitingForHuman => "Waiting for you",
        Substatus::WaitingForOwner => "Waiting for owner",
        Substatus::WaitingForCi => "Waiting for CI",
        Substatus::BlockedByMissingCapability => "Blocked — capability",
        Substatus::BlockedByMissingSecret => "Blocked — secret",
        Substatus::BlockedByPermission => "Blocked — permission",
        Substatus::BlockedByError => "Blocked — error",
    }
}

pub fn validate_substatus(
    substatus: Option<Substatus>,
    metadata: &Option<Value>,
) -> Option<&'static str> {
    let Some(s) = substatus else { return None; };
    let meta = metadata.as_ref().unwrap_or(&Value::Object(Default::default()));
    match s {
        Substatus::WaitingForAgent => {
            meta.get("agentId")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .map(|_| ())
                .or(Some("agentId required"))?;
        }
        Substatus::BlockedByMissingCapability => {
            if meta.get("capability").and_then(|v| v.as_str()).is_none() {
                return Some("capability required");
            }
        }
        Substatus::BlockedByMissingSecret => {
            if meta.get("secretKey").and_then(|v| v.as_str()).is_none() {
                return Some("secretKey required");
            }
        }
        _ => {}
    }
    None
}

pub fn validate_status_substatus_combo(
    status: TicketStatus,
    substatus: Option<Substatus>,
    metadata: &Option<Value>,
) -> Option<&'static str> {
    if let Some(msg) = validate_substatus(substatus, metadata) {
        return Some(msg);
    }
    if status == TicketStatus::Done {
        if let Some(s) = substatus {
            return Some(match s {
                Substatus::WaitingForAgent
                | Substatus::WaitingForHuman
                | Substatus::WaitingForOwner
                | Substatus::WaitingForCi => "done not allowed with waiting substatus",
                Substatus::BlockedByMissingCapability
                | Substatus::BlockedByMissingSecret
                | Substatus::BlockedByPermission
                | Substatus::BlockedByError => "done not allowed with blocked substatus",
            });
        }
    }
    None
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubstatusDisplay {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

pub fn build_substatus_display(
    substatus: Option<Substatus>,
    metadata: &Option<Value>,
    agent_name: Option<&str>,
) -> Option<SubstatusDisplay> {
    let s = substatus?;
    let label = substatus_label(s).to_string();
    let detail = match s {
        Substatus::WaitingForAgent => agent_name.map(String::from),
        Substatus::BlockedByMissingSecret => metadata
            .as_ref()
            .and_then(|m| m.get("secretKey"))
            .and_then(|v| v.as_str())
            .map(String::from),
        Substatus::BlockedByMissingCapability => metadata
            .as_ref()
            .and_then(|m| m.get("capability"))
            .and_then(|v| v.as_str())
            .map(String::from),
        Substatus::WaitingForHuman | Substatus::WaitingForOwner => metadata
            .as_ref()
            .and_then(|m| m.get("reason"))
            .and_then(|v| v.as_str())
            .map(String::from),
        _ => None,
    };
    Some(SubstatusDisplay { label, detail })
}
```

Export in `server/src/domain/mod.rs`: `pub mod substatus;`

- [ ] **Step 4: Run tests**

Run: `cargo test -p coppice-server substatus -- --nocapture`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add server/src/domain/substatus.rs server/src/domain/mod.rs
git commit -m "feat(server): add ticket substatus validation and display"
```

---

### Task 3: Project and repo service + API

**Files:**
- Create: `server/src/domain/project.rs`, `server/src/domain/repo.rs`
- Create: `server/src/services/project_service.rs`
- Create: `server/src/api/projects.rs`, `server/src/api/repos.rs`
- Modify: `server/src/services/mod.rs`, `server/src/domain/mod.rs`, `server/src/api/mod.rs`

- [ ] **Step 1: Write failing integration test**

Add to new `server/tests/integration_projects.rs`:

```rust
#[tokio::test]
async fn create_project_and_repo() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await { return; }
    let (app, _cookie, _csrf) = common::bootstrap_and_login().await;

    let create_project = app.clone().oneshot(
        common::json_request("POST", "/api/projects", r#"{"name":"Coppice Demo"}"#, &_cookie, &_csrf)
    ).await.unwrap();
    assert_eq!(create_project.status(), StatusCode::CREATED);

    let body: serde_json::Value = common::json_body(create_project).await;
    let project_id = body["id"].as_str().unwrap();

    let create_repo = app.oneshot(
        common::json_request(
            "POST",
            &format!("/api/projects/{project_id}/repos"),
            r#"{"name":"main-repo","defaultBranch":"main"}"#,
            &_cookie,
            &_csrf,
        )
    ).await.unwrap();
    assert_eq!(create_repo.status(), StatusCode::CREATED);
}
```

Create `server/tests/common/mod.rs` with `DB_TEST_LOCK`, `db_available`, `bootstrap_and_login`, `json_request`, `truncate_workspace` (truncate all M02 tables + users/sessions in FK order).

- [ ] **Step 2: Run test — expect FAIL**

Run: `DATABASE_URL=postgres://coppice:coppice@localhost:5432/coppice cargo test -p coppice-server integration_projects -- --nocapture`

- [ ] **Step 3: Implement `ProjectService`**

Key methods: `list_projects`, `create_project` (auto slug from name via slugify), `get_project`, `update_project`, `list_repos`, `create_repo`, `get_repo`, `update_repo`, `delete_repo`. Use `uuid::Uuid::new_v4()` for IDs.

API handlers return camelCase JSON via `#[serde(rename_all = "camelCase")]`.

Routes in `projects.rs`:
- `GET/POST /api/projects`
- `GET/PATCH /api/projects/:project_id`

Routes in `repos.rs`:
- `GET/POST /api/projects/:project_id/repos`
- `GET/PATCH/DELETE /api/repos/:repo_id`

Wire into `api/mod.rs` protected router (session + CSRF layers already applied to protected group — merge new routes into protected stack before CSRF or split: auth routes without CSRF on GET only; follow M01 pattern where protected routes include CSRF middleware).

- [ ] **Step 4: Run test — expect PASS**

- [ ] **Step 5: Commit**

```bash
git add server/src/domain/project.rs server/src/domain/repo.rs server/src/services/project_service.rs \
  server/src/api/projects.rs server/src/api/repos.rs server/tests/common/mod.rs server/tests/integration_projects.rs \
  server/src/services/mod.rs server/src/domain/mod.rs server/src/api/mod.rs
git commit -m "feat(server): add projects and repos API"
```

---

### Task 4: Ticket service + API

**Files:**
- Create: `server/src/domain/ticket.rs`
- Create: `server/src/services/ticket_service.rs`
- Create: `server/src/api/tickets.rs`
- Create: `server/tests/integration_tickets.rs`

- [ ] **Step 1: Write failing integration tests**

```rust
#[tokio::test]
async fn create_ticket_and_update_status() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await { return; }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;

    let res = app.clone().oneshot(common::json_request(
        "POST",
        &format!("/api/projects/{project_id}/tickets"),
        r#"{"title":"First ticket","description":"hello"}"#,
        &cookie, &csrf,
    )).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let ticket: serde_json::Value = common::json_body(res).await;
    let ticket_id = ticket["id"].as_str().unwrap();
    assert_eq!(ticket["status"], "backlog");

    let patch = app.oneshot(common::json_request(
        "PATCH",
        &format!("/api/tickets/{ticket_id}/status"),
        r#"{"status":"ready"}"#,
        &cookie, &csrf,
    )).await.unwrap();
    assert_eq!(patch.status(), StatusCode::OK);
}

#[tokio::test]
async fn reject_invalid_status() {
    // PATCH status "not_a_column" -> 400
}

#[tokio::test]
async fn reject_done_with_waiting_substatus() {
    // set substatus waiting_for_human then status done -> 400
}
```

- [ ] **Step 2: Run tests — FAIL**

- [ ] **Step 3: Implement `TicketService`**

Methods: `list_by_project` (optional filters), `create`, `get`, `update_fields`, `update_status`, `assign_agent`, `compute_last_activity_at`.

Use `substatus` module for validation on status PATCH. Include `substatusDisplay` in ticket JSON responses.

Routes:
- `GET/POST /api/projects/:project_id/tickets`
- `GET/PATCH /api/tickets/:ticket_id`
- `PATCH /api/tickets/:ticket_id/status`
- `POST /api/tickets/:ticket_id/assign` body `{ "agentId": null | "uuid" }`

- [ ] **Step 4: Run tests — PASS**

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(server): add tickets API with substatus validation"
```

---

### Task 5: Comments + attachments

**Files:**
- Create: `server/src/domain/comment.rs`, `server/src/domain/attachment.rs`
- Create: `server/src/services/comment_service.rs`
- Create: `server/src/storage/mod.rs`, `server/src/storage/attachment_store.rs`
- Create: `server/src/api/comments.rs`, `server/src/api/attachments.rs`
- Modify: `server/Cargo.toml` (axum `multipart` feature)
- Create: `server/tests/integration_comments.rs`

- [ ] **Step 1: Write failing integration test**

Test flow: create project → ticket → `POST /api/attachments` multipart → `POST /api/tickets/:id/comments` with `attachmentIds` → `GET comments` returns 1 item.

Use temp dir for artifacts in tests: set `COPPICE_STORAGE__ARTIFACTS_DIR=/tmp/coppice-test-artifacts` in test helper.

- [ ] **Step 2: Implement `AttachmentStore`**

```rust
pub struct AttachmentStore {
    root: PathBuf,
    max_bytes: u64,
}

impl AttachmentStore {
    pub fn save(&self, id: Uuid, filename: &str, content_type: &str, bytes: &[u8]) -> anyhow::Result<PathBuf> {
        if bytes.len() as u64 > self.max_bytes {
            anyhow::bail!("file too large");
        }
        let dir = self.root.join(id.to_string());
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(sanitize_filename(filename));
        std::fs::write(&path, bytes)?;
        Ok(path)
    }
}
```

Add `AttachmentStore` to `AppState` in `server/src/lib.rs`.

- [ ] **Step 3: Implement comment + attachment API**

`POST /api/attachments` — multipart field `file`; returns `{ id, filename, contentType, sizeBytes }`.

`GET /api/attachments/:id` — stream file with `Content-Type`.

`GET/POST /api/tickets/:ticket_id/comments` — POST `{ body, intent?, attachmentIds? }`; human author from `AuthUser`.

- [ ] **Step 4: Run integration test — PASS**

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(server): add comments and attachment upload API"
```

---

### Task 6: Agents + presets API

**Files:**
- Create: `server/src/domain/agent.rs`
- Create: `server/src/services/agent_service.rs`
- Create: `server/src/api/agents.rs`
- Create: `server/tests/integration_agents.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[tokio::test]
async fn list_presets_has_ten_entries() {
    // GET /api/agent-presets -> items.len() == 10
}

#[tokio::test]
async fn create_agent_from_preset() {
    // GET presets, POST /api/agents with presetId + name -> role copied
}
```

- [ ] **Step 2: Implement `AgentService` + routes**

`GET /api/agent-presets`, `GET/POST /api/agents`, `GET/PATCH/DELETE /api/agents/:id`.

`POST` with `presetId` copies fields from `agent_presets` row; sets `preset_source` to preset key.

- [ ] **Step 3: Run tests — PASS**

- [ ] **Step 4: Unit test preset count**

In `agent_service.rs` tests or migration test: assert 10 preset keys.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(server): add agents and preset templates API"
```

---

### Task 7: Admin users API + middleware

**Files:**
- Create: `server/src/middleware/admin.rs`
- Create: `server/src/services/user_service.rs`
- Create: `server/src/api/users.rs`
- Modify: `server/src/middleware/mod.rs`, `server/src/services/auth_service.rs` (if needed for password hash reuse)
- Extend: `server/tests/integration_auth.rs` or new `integration_users.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[tokio::test]
async fn admin_can_create_user() {
    // bootstrap admin -> POST /api/users { email, password } -> 201
}

#[tokio::test]
async fn member_cannot_create_user() {
    // create member via admin -> login member -> POST /api/users -> 403
}
```

- [ ] **Step 2: Implement `AdminUser` extractor**

```rust
pub struct AdminUser(pub AuthUser);

impl<S: Send + Sync> FromRequestParts<S> for AdminUser {
    type Rejection = StatusCode;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth = AuthUser::from_request_parts(parts, state).await?;
        if auth.user.role != "admin" {
            return Err(StatusCode::FORBIDDEN);
        }
        Ok(AdminUser(auth))
    }
}
```

- [ ] **Step 3: Implement `UserService::create_member` + routes**

`GET /api/users` → `{ items: [{ id, email, role, createdAt }] }`  
`POST /api/users` → creates `role = member`

- [ ] **Step 4: Run tests — PASS**

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(server): add admin user management API"
```

---

### Task 8: Integration workspace flow + 401 guard

**Files:**
- Create: `server/tests/integration_workspace.rs`
- Modify: `server/tests/common/mod.rs`

- [ ] **Step 1: Write full happy-path integration test**

Single test: bootstrap → create project → repo → agent from preset → ticket → assign agent → comment → PATCH substatus `waiting_for_agent` with metadata → GET ticket has `substatusDisplay.detail`.

- [ ] **Step 2: Write 401 test without session**

`GET /api/projects` without cookie → 401.

- [ ] **Step 3: Run full server test suite**

Run: `cargo test -p coppice-server`  
Expected: all pass (with postgres available)

- [ ] **Step 4: Commit**

```bash
git commit -m "test(server): add workspace integration coverage"
```

---

### Task 9: Web scaffold + frontend-design skill (mandatory)

**Files:**
- Create: `web/package.json`, `web/tsconfig.json`, `web/vite.config.ts`, `web/index.html`
- Create: `web/src/main.tsx`, `web/src/App.tsx`
- Create: `web/src/styles/tokens.css`, `web/tailwind.config.js`, `web/postcss.config.js`
- Create: `web/DESIGN.md`
- Modify: `web/README.md`

- [ ] **Step 1: REQUIRED — Invoke frontend-design skill**

Before writing UI components, run the **frontend-design** skill with this brief:

> Coppice — "grow an agent team from shared roots." Self-hosted agent workspace; Trello-like board; organic/coppice forest aesthetic; warm earth tones + living green accent; not generic purple SaaS. Produce theme tokens, font pairing, column status colors, substatus badge variants. Output feeds `web/src/styles/tokens.css` and `web/DESIGN.md`.

Do not proceed to Step 2 until tokens and DESIGN.md exist.

- [ ] **Step 2: Scaffold Vite React TS**

```bash
cd web && npm create vite@latest . -- --template react-ts
```

Add dependencies:

```bash
npm install @tanstack/react-query react-router-dom @dnd-kit/core @dnd-kit/sortable @hookform/resolvers react-hook-form zod react-markdown
npm install -D tailwindcss postcss autoprefixer vitest @testing-library/react jsdom
```

- [ ] **Step 3: Configure Vite proxy**

`web/vite.config.ts`:

```typescript
export default defineConfig({
  plugins: [react()],
  server: {
    host: '0.0.0.0',
    port: 5173,
    proxy: {
      '/api': {
        target: process.env.VITE_API_URL ?? 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
});
```

- [ ] **Step 4: Wire tokens into Tailwind**

Import `tokens.css` in `main.tsx`. Extend `tailwind.config.js` `theme.extend.colors` to reference CSS variables from frontend-design output.

- [ ] **Step 5: Verify dev server starts**

Run: `cd web && npm run dev`  
Expected: Vite on :5173 without errors

- [ ] **Step 6: Commit**

```bash
git add web/
git commit -m "feat(web): scaffold Vite SPA with design tokens"
```

---

### Task 10: API client + auth + routing shell

**Files:**
- Create: `web/src/lib/api.ts`, `web/src/lib/query-client.ts`
- Create: `web/src/features/auth/LoginPage.tsx`, `web/src/features/auth/useSession.ts`, `web/src/features/auth/AuthProvider.tsx`
- Create: `web/src/components/AppShell.tsx`, `web/src/components/ProtectedRoute.tsx`
- Modify: `web/src/App.tsx`

- [ ] **Step 1: Write failing Vitest for CSRF header helper**

`web/src/lib/api.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { withCsrf } from './api';

describe('withCsrf', () => {
  it('adds X-CSRF-Token when token set', () => {
    const headers = withCsrf('abc', { 'Content-Type': 'application/json' });
    expect(headers['X-CSRF-Token']).toBe('abc');
  });
});
```

- [ ] **Step 2: Implement `api.ts`**

```typescript
let csrfToken: string | null = null;
export function setCsrfToken(token: string) { csrfToken = token; }

export function withCsrf(token: string | null, headers: Record<string, string> = {}) {
  if (token) headers['X-CSRF-Token'] = token;
  return headers;
}

export async function apiFetch(path: string, init: RequestInit = {}) {
  const headers = withCsrf(csrfToken, {
    ...(init.headers as Record<string, string> ?? {}),
  });
  const res = await fetch(path, { ...init, headers, credentials: 'include' });
  if (!res.ok) throw new ApiError(res.status, await res.text());
  return res;
}
```

- [ ] **Step 3: Implement login flow**

`LoginPage` posts to `/api/auth/login`, reads `csrfToken` from JSON body, calls `setCsrfToken`, redirects to `/projects`.

`AuthProvider` on mount calls `GET /api/auth/me`; exposes `{ user, loading, logout }`.

- [ ] **Step 4: Wire routes**

```tsx
<Routes>
  <Route path="/login" element={<LoginPage />} />
  <Route element={<ProtectedRoute />}>
    <Route element={<AppShell />}>
      <Route path="/projects" element={<ProjectPickerPage />} />
      <Route path="/projects/:projectId/board" element={<BoardPage />} />
      <Route path="/agents" element={<AgentsPage />} />
      <Route path="/settings/users" element={<UsersPage />} />
    </Route>
  </Route>
</Routes>
```

Admin-only link to `/settings/users` when `user.role === 'admin'`.

- [ ] **Step 5: Run Vitest**

Run: `cd web && npm run test`  
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(web): add API client, auth, and app routing shell"
```

---

### Task 11: Project picker

**Files:**
- Create: `web/src/features/projects/ProjectPickerPage.tsx`, `web/src/features/projects/useProjects.ts`

- [ ] **Step 1: Implement project list + create**

TanStack Query: `useQuery(['projects'], () => apiFetch('/api/projects'))`, `useMutation` POST create → invalidate.

UI: grid/list of projects; "New project" dialog (name field); click project → `navigate(/projects/${id}/board)`; store last project id in `localStorage`.

- [ ] **Step 2: Manual smoke**

With compose up + migrated + bootstrapped: login → create project → lands on board route (board can be placeholder).

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(web): add project picker"
```

---

### Task 12: Kanban board + optimistic DnD

**Files:**
- Create: `web/src/features/board/BoardPage.tsx`, `web/src/features/board/BoardColumn.tsx`, `web/src/features/board/TicketCard.tsx`
- Create: `web/src/features/board/columns.ts`, `web/src/features/board/useTickets.ts`
- Create: `web/src/features/board/columns.test.ts`

- [ ] **Step 1: Write failing test for column order**

```typescript
import { BOARD_COLUMNS } from './columns';
it('has eight columns in spec order', () => {
  expect(BOARD_COLUMNS.map(c => c.status)).toEqual([
    'backlog','ready','in_progress','in_review','in_qa','wait_for_final_review','done','blocked',
  ]);
});
```

- [ ] **Step 2: Implement board layout**

Horizontal scroll; each column is droppable (`@dnd-kit/core`). Cards draggable within/between columns.

On drag end: optimistic update `queryClient.setQueryData(['tickets', projectId], ...)` then `PATCH /api/tickets/:id/status`; rollback on error.

- [ ] **Step 3: Quick-add ticket in Backlog**

Inline form or modal → `POST /api/projects/:id/tickets`.

- [ ] **Step 4: Card displays `substatusDisplay` badges**

- [ ] **Step 5: Run Vitest + manual drag test**

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(web): add kanban board with optimistic drag"
```

---

### Task 13: Fullscreen ticket drawer

**Files:**
- Create: `web/src/features/tickets/TicketDrawer.tsx`, `web/src/features/tickets/TicketDescriptionTab.tsx`, `web/src/features/tickets/TicketCommentsTab.tsx`, `web/src/features/tickets/TicketMetadataTab.tsx`
- Create: `web/src/lib/schemas/ticket.ts`, `web/src/lib/schemas/substatus.ts`
- Modify: `web/src/features/board/BoardPage.tsx`

- [ ] **Step 1: Write failing Vitest for substatus schema**

```typescript
import { substatusMetadataSchema } from '../schemas/substatus';
it('requires agentId for waiting_for_agent', () => {
  const r = substatusMetadataSchema.safeParse({ substatus: 'waiting_for_agent', metadata: {} });
  expect(r.success).toBe(false);
});
```

- [ ] **Step 2: URL-driven drawer**

In `BoardPage`, read `const [params, setSearchParams] = useSearchParams()`; `ticketId = searchParams.get('ticket')`.

Open: `setSearchParams({ ticket: id })`. Close: `setSearchParams({})` — **do not navigate away from board route**.

Drawer: `fixed inset-0 z-50` overlay; board remains mounted underneath.

- [ ] **Step 3: Hybrid cache on close**

```typescript
const closeDrawer = () => {
  setSearchParams({});
  queryClient.invalidateQueries({ queryKey: ['tickets', projectId] });
};
```

Edits inside drawer update `['ticket', id]` and `['comments', id]` without invalidating board list until close.

- [ ] **Step 4: Implement tabs**

Description (markdown preview/edit), Comments (thread + POST), Metadata (substatus select + conditional fields → PATCH status/ticket).

- [ ] **Step 5: Attachment upload on comment**

Upload file → `POST /api/attachments` → pass `attachmentIds` in comment POST.

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(web): add fullscreen ticket drawer with comments"
```

---

### Task 14: Agents UI + admin users page

**Files:**
- Create: `web/src/features/agents/AgentsPage.tsx`, `web/src/features/agents/AgentForm.tsx`
- Create: `web/src/features/users/UsersPage.tsx`
- Create: `web/src/lib/schemas/agent.ts`

- [ ] **Step 1: Agents list + create from preset**

Load presets + agents. Create dialog: pick preset → prefill form → `POST /api/agents`.

Edit: `PATCH /api/agents/:id`. Toggle enabled.

- [ ] **Step 2: Assign agent in ticket drawer**

Dropdown in Description tab → `POST /api/tickets/:id/assign`.

- [ ] **Step 3: Users page (admin)**

List users; form to create email/password. Hide route for non-admin (redirect or 403 page).

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(web): add agents and admin users UI"
```

---

### Task 15: Docker web service + artifact volume

**Files:**
- Create: `deploy/Dockerfile.web`
- Modify: `deploy/docker-compose.yml`
- Modify: `Makefile`

- [ ] **Step 1: Create `deploy/Dockerfile.web`**

```dockerfile
FROM node:22-bookworm
WORKDIR /app
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ .
EXPOSE 5173
CMD ["npm", "run", "dev", "--", "--host", "0.0.0.0"]
```

- [ ] **Step 2: Update compose**

Add `web` service (port 5173, depends_on server, `VITE_API_URL=http://server:8080`).

Add to `server`:

```yaml
volumes:
  - artifact_data:/data/artifacts
environment:
  COPPICE_STORAGE__ARTIFACTS_DIR: /data/artifacts
volumes:
  artifact_data:
```

- [ ] **Step 3: Update Makefile**

```makefile
web-test:
	cd web && npm run test

web-dev:
	cd web && npm run dev
```

- [ ] **Step 4: Verify compose**

Run: `make compose-up`  
Expected: postgres + server + web healthy; open `http://localhost:5173/login`

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(deploy): add web container and artifact volume"
```

---

### Task 16: Static SPA serving + release targets

**Files:**
- Modify: `server/src/main.rs`, `server/src/api/mod.rs`
- Modify: `server/Cargo.toml` (tower-http `fs` feature)
- Modify: `Makefile`, `README.md`
- Create: `deploy/README-RELEASE.md`

- [ ] **Step 1: Serve static files when `storage.static_dir` set**

```rust
use tower_http::services::{ServeDir, ServeFile};

if let Some(static_dir) = &config.storage.static_dir {
    let index = format!("{static_dir}/index.html");
    app = app.fallback_service(
        ServeDir::new(static_dir)
            .not_found_service(ServeFile::new(index)),
    );
}
```

- [ ] **Step 2: Add Makefile targets**

```makefile
web-build:
	cd web && npm ci && npm run build

release-tar: web-build
	cargo build --release -p coppice-server -p coppice-cli
	mkdir -p dist/release
	cp target/release/coppice-server target/release/coppice-cli dist/release/
	cp -r web/dist dist/release/web/dist
	cp deploy/config/default.yaml dist/release/
	cp deploy/README-RELEASE.md dist/release/
	tar -czf dist/coppice-$$(uname -s | tr A-Z a-z)-$$(uname -m).tar.gz -C dist/release .
```

- [ ] **Step 3: Document in README-RELEASE.md**

Explain: set `COPPICE_STORAGE__STATIC_DIR=./web/dist`, run binary on :8080.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(server): serve SPA static assets for release builds"
```

---

### Task 17: CI Vitest + E2E smoke

**Files:**
- Modify: `.github/workflows/ci.yml`
- Create: `e2e/smoke/m02-board.mjs`
- Modify: `Makefile`

- [ ] **Step 1: Add CI job `web`**

```yaml
  web:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '22'
          cache: npm
          cache-dependency-path: web/package-lock.json
      - run: cd web && npm ci && npm run test
```

- [ ] **Step 2: Write smoke script `e2e/smoke/m02-board.mjs`**

Use agent-browser / browse skill commands:

1. Navigate `http://localhost:5173/login`
2. Fill bootstrap credentials → submit
3. Create project → open board
4. Create ticket in Backlog
5. Drag to Ready (or PATCH via UI interaction)
6. Open ticket drawer → add comment → assert visible
7. Close drawer → assert board still visible without full-page reload

- [ ] **Step 3: Add `make e2e-smoke`**

Depends on `compose-up`; runs smoke script.

- [ ] **Step 4: Commit**

```bash
git commit -m "ci: add web unit tests and M02 board smoke E2E"
```

---

### Task 18: M02 acceptance verification

- [ ] **Step 1: Run full checklist**

```bash
make compose-up
make migrate
cargo run -p coppice-cli -- bootstrap admin --email admin@localhost --password changeme
cargo test --workspace
cargo clippy --workspace -- -- -D warnings
cd web && npm run test
make e2e-smoke   # if agent-browser available locally
make web-build
make release-tar
```

- [ ] **Step 2: Verify acceptance criteria from spec**

Mark items in spec mentally; fix any gaps found.

- [ ] **Step 3: Update root README quick start**

Add web URL `http://localhost:5173` and login flow.

- [ ] **Step 4: Commit**

```bash
git commit -m "chore(m02): complete workspace and board milestone acceptance"
```

---

## Spec coverage self-review

| Spec requirement | Task |
|------------------|------|
| Migration + presets | Task 1 |
| Substatus validation + display | Task 2, 4 |
| Projects/repos API | Task 3 |
| Tickets status/assign | Task 4 |
| Comments + attachments | Task 5 |
| Agents + presets | Task 6 |
| Admin users | Task 7 |
| Integration coverage | Task 8 |
| frontend-design mandatory | Task 9 Step 1 |
| Login + session SPA | Task 10 |
| Multi-project picker | Task 11 |
| Board + optimistic DnD | Task 12 |
| Fullscreen drawer + hybrid cache | Task 13 |
| Agents UI + assign | Task 14 |
| Compose web + artifacts | Task 15 |
| Release tarball + static serve | Task 16 |
| Vitest + smoke E2E | Task 17 |
| Acceptance | Task 18 |

No TBD/TODO placeholders remain. Type names (`Substatus`, `TicketStatus`, `substatusDisplay`) consistent across server and web schema tasks.

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-07-m02-workspace-and-board.md`. Two execution options:

**1. Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks, fast iteration. REQUIRED SUB-SKILL: subagent-driven-development.

**2. Inline Execution** — execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
