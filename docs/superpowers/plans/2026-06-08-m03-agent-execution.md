# M03 Agent Execution Implementation Plan

> **Note:** Repository bootstrap sections in this plan are **obsolete** — superseded by [2026-06-08-m03-registered-repositories.md](./2026-06-08-m03-registered-repositories.md) (registered `local_path` repos, no lazy clone). Job queue, worker, worktrees, MockProvider, and Runs UI steps remain valid.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver Postgres-backed agent job queue, background worker, git worktrees, MockProvider runs, result-contract-driven ticket updates, agent comments, and ticket drawer Runs tab with header Run Agent / Stop actions.

**Architecture:** In-process Tokio worker(s) poll `agent_jobs` with `FOR UPDATE SKIP LOCKED`, execute clone → worktree → context → provider → apply pipeline. One worktree per `(ticket, agent)`; lazy git clone; strict run preconditions; result contract updates ticket status/substatus and creates agent comments (mentions on comment only).

**Tech Stack:** Rust/Axum/SQLx/tokio::process, React/Vite/TanStack Query, git CLI, Docker Compose, Vitest, Node smoke E2E

**Spec:** [docs/superpowers/specs/2026-06-08-m03-agent-execution-design.md](../specs/2026-06-08-m03-agent-execution-design.md)

---

## File map (created or modified in M03)

| Path | Responsibility |
|------|----------------|
| `server/migrations/003_agent_execution.sql` | `agent_runs`, `agent_jobs` tables + indexes |
| `server/src/domain/run.rs` | Run status enum, `AgentRun` model |
| `server/src/domain/job.rs` | Job status enum, `AgentJob` model |
| `server/src/domain/slug.rs` | Slug sanitization for paths/branches |
| `server/src/services/worktree_service.rs` | Lazy clone, worktree add/reuse, git CLI |
| `server/src/services/context_builder.rs` | `.agent/context.md` generation |
| `server/src/services/result_contract.rs` | Parse result, map status/substatus, build comment body |
| `server/src/services/run_service.rs` | Run CRUD, start/stop/retry, apply results |
| `server/src/services/job_service.rs` | Job enqueue, claim, complete, cancel |
| `server/src/workers/job_worker.rs` | Poll loop + pipeline orchestration |
| `server/src/workers/mod.rs` | Module root |
| `server/src/sandbox/permissive.rs` | Placeholder sandbox profile id + note text |
| `server/src/sandbox/mod.rs` | Module root |
| `server/src/providers/mock.rs` | Optional stdout artifact file |
| `server/src/providers/mod.rs` | Extend `AgentRunResult::Blocked` fields |
| `server/src/api/agent_runs.rs` | Run detail, stop, retry routes |
| `server/src/api/jobs.rs` | Admin debug job list |
| `server/src/api/tickets.rs` | + `POST run-agent`, `GET runs` |
| `server/src/api/mod.rs` | Wire new routes |
| `server/src/config/mod.rs` | + `AgentConfig` (paths, worker count, provider id) |
| `server/src/lib.rs` | + `workers`, `sandbox`; extend `AppState` |
| `server/src/main.rs` | Spawn worker tasks on startup |
| `server/src/services/comment_service.rs` | + `mentions` param on `create` |
| `server/tests/common/mod.rs` | Truncate runs/jobs; temp git repo helper |
| `server/tests/integration_agent_runs.rs` | Full run pipeline integration tests |
| `deploy/Dockerfile.server` | Install `git` |
| `deploy/docker-compose.yml` | + repo/worktree volumes, agent env |
| `deploy/docker-compose.local.yml` | Same volume/env delta |
| `deploy/config/default.yaml` | + `agent` section |
| `fixtures/agent-responses/blocked-missing-capability.json` | Blocked fixture variant for tests |
| `web/src/lib/schemas/agentRun.ts` | Zod schema for AgentRun |
| `web/src/features/tickets/useAgentRuns.ts` | Query + mutations |
| `web/src/features/tickets/TicketRunsTab.tsx` | Read-only runs list |
| `web/src/features/tickets/TicketDrawer.tsx` | Runs tab + header Run/Stop |
| `web/src/features/tickets/TicketMetadataTab.tsx` | Show branch + worktree |
| `e2e/smoke/m03-agent-run.mjs` | CI smoke script |
| `Makefile` | Extend `e2e-smoke` or add `e2e-smoke-m03` |
| `AGENTS.md`, `docs/architecture.md` | M03 pointers |

---

### Task 1: Migration + domain types + agent config

**Files:**
- Create: `server/migrations/003_agent_execution.sql`
- Create: `server/src/domain/run.rs`
- Create: `server/src/domain/job.rs`
- Modify: `server/src/domain/mod.rs`
- Modify: `server/src/config/mod.rs`
- Modify: `deploy/config/default.yaml`

- [ ] **Step 1: Write migration `003_agent_execution.sql`**

```sql
CREATE TABLE agent_runs (
    id UUID PRIMARY KEY,
    ticket_id UUID NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    job_type TEXT NOT NULL,
    status TEXT NOT NULL,
    sandbox_profile_id TEXT NOT NULL,
    worktree_path TEXT,
    branch_name TEXT,
    error_message TEXT,
    started_at TIMESTAMPTZ,
    ended_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX agent_runs_ticket_id_created_at_idx
    ON agent_runs (ticket_id, created_at DESC);
CREATE INDEX agent_runs_agent_id_idx ON agent_runs (agent_id);

CREATE UNIQUE INDEX agent_runs_active_ticket_agent_idx
    ON agent_runs (ticket_id, agent_id)
    WHERE status IN ('queued', 'running');

CREATE TABLE agent_jobs (
    id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    job_type TEXT NOT NULL,
    status TEXT NOT NULL,
    attempts INT NOT NULL DEFAULT 0,
    max_attempts INT NOT NULL DEFAULT 3,
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_at TIMESTAMPTZ,
    locked_by TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX agent_jobs_pending_idx
    ON agent_jobs (status, available_at)
    WHERE status = 'pending';
```

- [ ] **Step 2: Add domain types**

`server/src/domain/run.rs`:

```rust
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct AgentRun {
    pub id: Uuid,
    pub ticket_id: Uuid,
    pub agent_id: Uuid,
    pub job_type: String,
    pub status: RunStatus,
    pub sandbox_profile_id: String,
    pub worktree_path: Option<String>,
    pub branch_name: Option<String>,
    pub error_message: Option<String>,
    pub started_at: Option<OffsetDateTime>,
    pub ended_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

pub fn run_status_to_str(s: RunStatus) -> &'static str { /* queued, running, … */ }
pub fn run_status_from_str(s: &str) -> Option<RunStatus> { /* … */ }
```

`server/src/domain/job.rs` — mirror pattern for `JobStatus` (`pending`, `processing`, `done`, `failed`, `cancelled`) and `AgentJob` struct.

Export both from `server/src/domain/mod.rs`.

- [ ] **Step 3: Extend config with `AgentConfig`**

In `server/src/config/mod.rs`:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentConfig {
    pub default_provider: String,
    pub git_repos_path: String,
    pub worktrees_path: String,
    pub worker_count: u32,
}

// Add to AppConfig:
pub agent: AgentConfig,

// default_values():
agent: AgentConfig {
    default_provider: "mock".into(),
    git_repos_path: "./data/repos".into(),
    worktrees_path: "./data/worktrees".into(),
    worker_count: 2,
},
```

Merge env: `AGENT_DEFAULT_PROVIDER`, `GIT_REPOS_PATH`, `WORKTREES_PATH`, `AGENT_WORKER_COUNT` via existing `COPPICE_` prefix or raw env merges (match compose env names from spec).

Add to `deploy/config/default.yaml`:

```yaml
agent:
  default_provider: mock
  git_repos_path: /data/repos
  worktrees_path: /data/worktrees
  worker_count: 2
```

- [ ] **Step 4: Run migration locally**

Run: `make migrate` (or `make migrate-local` if using local stack)

Expected: migration applies without error.

- [ ] **Step 5: Commit**

```bash
git add server/migrations/003_agent_execution.sql server/src/domain/run.rs server/src/domain/job.rs server/src/domain/mod.rs server/src/config/mod.rs deploy/config/default.yaml
git commit -m "feat(server): add agent runs/jobs migration and config"
```

---

### Task 2: Slug utility + worktree service

**Files:**
- Create: `server/src/domain/slug.rs`
- Create: `server/src/services/worktree_service.rs`
- Modify: `server/src/services/mod.rs`
- Modify: `server/src/domain/mod.rs`

- [ ] **Step 1: Write failing slug tests**

Add to `server/src/domain/slug.rs`:

```rust
pub fn slugify(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut out = String::new();
    let mut prev_hyphen = false;
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_hyphen = false;
        } else if !prev_hyphen {
            out.push('-');
            prev_hyphen = true;
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_agent_name() {
        assert_eq!(slugify("Frontend Engineer"), "frontend-engineer");
    }

    #[test]
    fn slugify_collapses_separators() {
        assert_eq!(slugify("foo---bar"), "foo-bar");
    }
}
```

- [ ] **Step 2: Run slug tests**

Run: `cargo test -p coppice-server slugify -- --nocapture`

Expected: PASS

- [ ] **Step 3: Write failing worktree path tests**

`server/src/services/worktree_service.rs`:

```rust
pub struct WorktreePaths {
    pub repo_dir: PathBuf,
    pub worktree_dir: PathBuf,
    pub branch_name: String,
}

pub fn compute_paths(
    repos_root: &Path,
    worktrees_root: &Path,
    repo_id: Uuid,
    repo_name: &str,
    ticket_id: Uuid,
    agent_name: &str,
) -> WorktreePaths {
    let agent_slug = crate::domain::slug::slugify(agent_name);
    let repo_slug = crate::domain::slug::slugify(repo_name);
    let ticket_short = ticket_id.to_string().split('-').next().unwrap_or("ticket");
    WorktreePaths {
        repo_dir: repos_root.join(repo_id.to_string()),
        worktree_dir: worktrees_root.join(format!(
            "TICKET-{ticket_short}-{agent_slug}-{repo_slug}"
        )),
        branch_name: format!("agent/TICKET-{ticket_short}-{agent_slug}"),
    }
}
```

Unit test asserts path and branch strings for known UUID/name inputs.

- [ ] **Step 4: Implement git operations**

Add methods on `WorktreeService`:

```rust
pub async fn ensure_repo_clone(&self, remote_url: &str, repo_dir: &Path) -> Result<(), WorktreeError>
pub async fn ensure_worktree(
    &self,
    repo_dir: &Path,
    worktree_dir: &Path,
    branch: &str,
) -> Result<(), WorktreeError>
```

Use `tokio::process::Command` for `git clone`, `git worktree add -b {branch} {path}` (if path missing), skip if worktree dir exists.

- [ ] **Step 5: Run worktree unit tests**

Run: `cargo test -p coppice-server worktree -- --nocapture`

Expected: PASS (path tests always; git tests use `#[ignore]` unless git available — add one integration-style test in Task 8).

- [ ] **Step 6: Commit**

```bash
git add server/src/domain/slug.rs server/src/services/worktree_service.rs server/src/services/mod.rs server/src/domain/mod.rs
git commit -m "feat(server): add slug helper and worktree service"
```

---

### Task 3: Context builder

**Files:**
- Create: `server/src/services/context_builder.rs`
- Modify: `server/src/services/mod.rs`

- [ ] **Step 1: Write failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_includes_required_sections() {
        let md = build_context_md(&ContextInput {
            ticket_title: "Fix polling",
            ticket_description: "Add retry",
            ticket_status: "in_progress",
            ticket_substatus: None,
            agent_name: "FE Agent",
            agent_role: "Frontend Engineer",
            agent_skills: &["react".into()],
            agent_responsibilities: &["implement UI".into()],
            agent_system_prompt: "You are FE.",
        });
        assert!(md.contains("# Current task"));
        assert!(md.contains("# Agent role"));
        assert!(md.contains("# Sandbox"));
        assert!(md.contains("# Expected output contract"));
        assert!(md.contains("Fix polling"));
    }
}
```

- [ ] **Step 2: Run test — expect FAIL**

Run: `cargo test -p coppice-server context_includes -- --nocapture`

- [ ] **Step 3: Implement `build_context_md` + `write_context_file`**

Write markdown to `{worktree}/.agent/context.md` (create `.agent` dir). Include sandbox note from `sandbox::permissive::SANDBOX_NOTE` and JSON contract summary for `done`/`blocked`.

- [ ] **Step 4: Run test — expect PASS**

- [ ] **Step 5: Commit**

```bash
git add server/src/services/context_builder.rs server/src/services/mod.rs
git commit -m "feat(server): add agent context package builder"
```

---

### Task 4: Result contract parser

**Files:**
- Create: `server/src/services/result_contract.rs`
- Modify: `server/src/providers/mod.rs`
- Modify: `server/src/services/mod.rs`

- [ ] **Step 1: Extend `AgentRunResult::Blocked` in `providers/mod.rs`**

Add optional fields matching product design §17:

```rust
Blocked {
    #[serde(rename = "blockerType")]
    blocker_type: String,
    summary: String,
    #[serde(rename = "nextStatus")]
    next_status: String,
    #[serde(rename = "mentionAgents")]
    mention_agents: Vec<String>,
    #[serde(default, rename = "requiredCapabilities")]
    required_capabilities: Vec<String>,
    #[serde(default, rename = "requiredSecrets")]
    required_secrets: Vec<String>,
},
```

- [ ] **Step 2: Write failing result_contract tests**

`server/src/services/result_contract.rs`:

```rust
pub struct ApplyTicketUpdate {
    pub status: TicketStatus,
    pub substatus: Option<Substatus>,
    pub substatus_metadata: Option<serde_json::Value>,
}

pub struct ApplyComment {
    pub body: String,
    pub intent: CommentIntent,
    pub mentions: Vec<String>,
}

pub struct ApplyResult {
    pub run_status: RunStatus,
    pub ticket: ApplyTicketUpdate,
    pub comment: ApplyComment,
}

pub fn ticket_status_from_next_status(label: &str) -> Option<TicketStatus> {
    match label.trim() {
        "Backlog" | "backlog" => Some(TicketStatus::Backlog),
        "Ready" | "ready" => Some(TicketStatus::Ready),
        "In Progress" | "in_progress" => Some(TicketStatus::InProgress),
        "In Review" | "in_review" => Some(TicketStatus::InReview),
        "In QA" | "in_qa" => Some(TicketStatus::InQa),
        "Wait for Final Review" | "wait_for_final_review" => Some(TicketStatus::WaitForFinalReview),
        "Done" | "done" => Some(TicketStatus::Done),
        "Blocked" | "blocked" => Some(TicketStatus::Blocked),
        _ => None,
    }
}

pub fn apply_agent_result(result: &AgentRunResult) -> Result<ApplyResult, String> { /* … */ }
```

Tests:
- `done.json` fixture → `RunStatus::Succeeded`, status `InReview`, intent `implementation_done`
- `blocked.json` → `RunStatus::Blocked`, status `Blocked`, intent `blocked`
- blocked with `blockerType: missing_capability` + `requiredCapabilities: ["postgres"]` → substatus `BlockedByMissingCapability`

- [ ] **Step 3: Run tests — implement until PASS**

Run: `cargo test -p coppice-server result_contract -- --nocapture`

- [ ] **Step 4: Commit**

```bash
git add server/src/services/result_contract.rs server/src/providers/mod.rs server/src/services/mod.rs
git commit -m "feat(server): add result contract parser and ticket apply mapping"
```

---

### Task 5: Sandbox placeholder + comment mentions

**Files:**
- Create: `server/src/sandbox/mod.rs`
- Create: `server/src/sandbox/permissive.rs`
- Modify: `server/src/lib.rs`
- Modify: `server/src/services/comment_service.rs`

- [ ] **Step 1: Add permissive sandbox module**

```rust
// server/src/sandbox/permissive.rs
pub const PROFILE_ID: &str = "permissive-default";
pub const SANDBOX_NOTE: &str = "Permissive sandbox (M03 placeholder). If you need a command, secret, or path that is not available, return a blocked result — do not guess.";
```

- [ ] **Step 2: Extend `CommentService::create` to accept mentions**

```rust
pub async fn create(
    &self,
    ticket_id: Uuid,
    author_type: AuthorType,
    author_id: Option<Uuid>,
    body: &str,
    intent: CommentIntent,
    attachment_ids: &[Uuid],
    mentions: &[String],
) -> Result<Comment, CommentError>
```

Bind `mentions` as `serde_json::json!(mentions)` instead of empty array. Update existing API handler in `api/comments.rs` to pass `&[]` or parse from request body if mentions field exists.

- [ ] **Step 3: Run workspace tests**

Run: `cargo test -p coppice-server -- --nocapture`

Expected: all existing tests PASS

- [ ] **Step 4: Commit**

```bash
git add server/src/sandbox server/src/lib.rs server/src/services/comment_service.rs server/src/api/comments.rs
git commit -m "feat(server): add permissive sandbox stub and comment mentions"
```

---

### Task 6: Run service + job service

**Files:**
- Create: `server/src/services/run_service.rs`
- Create: `server/src/services/job_service.rs`

- [ ] **Step 1: Implement `JobService`**

Key methods:

```rust
impl JobService<'_> {
    pub async fn enqueue(&self, run_id: Uuid, job_type: &str) -> Result<AgentJob, JobError>;
    pub async fn claim_next(&self, worker_id: &str) -> Result<Option<AgentJob>, JobError>;
    pub async fn mark_done(&self, job_id: Uuid) -> Result<(), JobError>;
    pub async fn mark_failed(&self, job_id: Uuid, message: &str) -> Result<(), JobError>;
    pub async fn cancel_for_run(&self, run_id: Uuid) -> Result<(), JobError>;
}
```

`claim_next` SQL pattern:

```sql
UPDATE agent_jobs
SET status = 'processing', locked_at = now(), locked_by = $1, attempts = attempts + 1
WHERE id = (
    SELECT id FROM agent_jobs
    WHERE status = 'pending' AND available_at <= now()
    ORDER BY available_at ASC
    FOR UPDATE SKIP LOCKED
    LIMIT 1
)
RETURNING id, run_id, job_type, status, attempts, max_attempts, available_at, locked_at, locked_by, created_at;
```

- [ ] **Step 2: Implement `RunService`**

Key methods:

```rust
impl RunService<'_> {
    pub async fn start_run(&self, ticket_id: Uuid) -> Result<AgentRun, RunError>;
    pub async fn get(&self, run_id: Uuid) -> Result<AgentRun, RunError>;
    pub async fn list_for_ticket(&self, ticket_id: Uuid) -> Result<Vec<AgentRun>, RunError>;
    pub async fn stop(&self, run_id: Uuid) -> Result<AgentRun, RunError>;
    pub async fn retry(&self, run_id: Uuid) -> Result<AgentRun, RunError>;
    pub async fn mark_running(&self, run_id: Uuid) -> Result<(), RunError>;
    pub async fn finish_with_apply(&self, run_id: Uuid, apply: ApplyResult, worktree_path: Option<String>, branch_name: Option<String>) -> Result<AgentRun, RunError>;
    pub async fn finish_failed(&self, run_id: Uuid, message: &str) -> Result<AgentRun, RunError>;
    pub async fn is_cancelled(&self, run_id: Uuid) -> Result<bool, RunError>;
}
```

`start_run` validates:
- ticket exists, has `assignee_agent_id`, has `repo_id`
- repo has non-empty `remote_url`
- no active run for `(ticket_id, assignee_agent_id)` → else `RunError::ActiveRunExists` (maps to 409)

Creates run (`queued`, `sandbox_profile_id = permissive-default`) + enqueues job.

`finish_with_apply` uses `TicketService::update_status` + `CommentService::create` with agent author.

- [ ] **Step 3: Unit test job status helpers**

Test `run_status_from_str` / `job_status_from_str` roundtrips.

- [ ] **Step 4: Commit**

```bash
git add server/src/services/run_service.rs server/src/services/job_service.rs server/src/services/mod.rs
git commit -m "feat(server): add run and job services"
```

---

### Task 7: Job worker + AppState + startup

**Files:**
- Create: `server/src/workers/mod.rs`
- Create: `server/src/workers/job_worker.rs`
- Modify: `server/src/lib.rs`
- Modify: `server/src/main.rs`
- Modify: `server/src/providers/mock.rs`

- [ ] **Step 1: Extend `AppState`**

```rust
pub struct AppState {
    pub config: AppConfig,
    pub db: Option<PgPool>,
    pub attachments: AttachmentStore,
    pub agent_provider: Arc<dyn crate::providers::AgentProvider>,
}

impl AppState {
    pub fn agent_provider_from_config(config: &AppConfig) -> Arc<dyn crate::providers::AgentProvider> {
        match config.agent.default_provider.as_str() {
            "mock" => Arc::new(crate::providers::mock::MockProvider::default()),
            other => panic!("unknown agent provider: {other}"),
        }
    }
}
```

Update `test_state`, `main.rs`, and `server/tests/common/mod.rs` to populate `agent_provider`.

- [ ] **Step 2: Implement worker loop**

`server/src/workers/job_worker.rs`:

```rust
pub fn spawn_workers(state: Arc<AppState>) {
    let count = state.config.agent.worker_count.max(1);
    for i in 0..count {
        let state = state.clone();
        tokio::spawn(async move {
            let worker_id = format!("worker-{i}");
            loop {
                if let Err(err) = process_one(&state, &worker_id).await {
                    tracing::error!(%err, "job worker error");
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });
    }
}

async fn process_one(state: &AppState, worker_id: &str) -> anyhow::Result<()> {
    let pool = state.db.as_ref().context("no db")?;
    let job = JobService::new(pool).claim_next(worker_id).await?;
    let Some(job) = job else { return Ok(()); };

    // load run, ticket, agent, repo
    // if run cancelled → mark job cancelled, return
    // worktree pipeline
    // check cancelled again before apply
    // provider.run(AgentRunInput { context_path, agent_id, ticket_id })
    // result_contract::apply_agent_result
    // run_service.finish_with_apply or finish_failed
    Ok(())
}
```

Wire `WorktreeService`, `context_builder`, `result_contract` inside `process_one`.

- [ ] **Step 3: Spawn workers in `main.rs`**

After building `Arc<AppState>`:

```rust
coppice_server::workers::job_worker::spawn_workers(state.clone());
```

- [ ] **Step 4: Manual smoke**

Run server with compose up, POST run-agent via curl (after Task 9 API exists) — for now run `cargo test -p coppice-server`.

- [ ] **Step 5: Commit**

```bash
git add server/src/workers server/src/lib.rs server/src/main.rs server/tests/common/mod.rs
git commit -m "feat(server): add in-process agent job worker"
```

---

### Task 8: Integration tests

**Files:**
- Create: `server/tests/integration_agent_runs.rs`
- Modify: `server/tests/common/mod.rs`

- [ ] **Step 1: Add temp git repo helper**

In `server/tests/common/mod.rs`:

```rust
pub fn create_temp_git_remote() -> (tempfile::TempDir, String) {
    use std::process::Command;
    let bare = tempfile::tempdir().expect("tempdir");
    Command::new("git")
        .args(["init", "--bare"])
        .current_dir(bare.path())
        .output()
        .expect("git init --bare");
    let url = format!("file://{}", bare.path().display());
    (bare, url)
}
```

Add `tempfile` to workspace `Cargo.toml` dev-dependencies if not present.

Update `truncate_workspace` to include `agent_jobs`, `agent_runs` before tickets.

- [ ] **Step 2: Write integration test — happy path**

`server/tests/integration_agent_runs.rs`:

```rust
#[tokio::test]
async fn run_agent_applies_done_fixture() {
    if !common::db_available().await { return; }
    let _lock = common::DB_TEST_LOCK.lock().await;
    // bootstrap_and_login
    // create project, repo with file:// remote, agent, ticket with assignee + repo
    // set env MOCK_AGENT_RESPONSE=done
    // POST /api/tickets/{id}/run-agent
    // poll GET /api/tickets/{id}/runs until succeeded (timeout 10s)
    // assert ticket status in_review
    // assert agent comment with implementation_done
}
```

Use `std::env::set_var("MOCK_AGENT_RESPONSE", "done")` in test; set `GIT_REPOS_PATH` and `WORKTREES_PATH` to temp dirs via env before `test_state_with_db`.

- [ ] **Step 3: Add blocked, duplicate, stop, retry tests**

Follow spec matrix:
- `MOCK_AGENT_RESPONSE=blocked` → blocked comment + ticket Blocked
- second run-agent while first active → 409
- stop on queued run → cancelled
- retry after failed → new run id

- [ ] **Step 4: Run integration tests**

Run: `DATABASE_URL=postgres://coppice:coppice@localhost:5432/coppice cargo test -p coppice-server --test integration_agent_runs -- --nocapture`

Expected: PASS (requires Postgres + git in PATH)

- [ ] **Step 5: Commit**

```bash
git add server/tests/integration_agent_runs.rs server/tests/common/mod.rs Cargo.toml
git commit -m "test(server): add agent run integration tests"
```

---

### Task 9: API routes

**Files:**
- Create: `server/src/api/agent_runs.rs`
- Create: `server/src/api/jobs.rs`
- Modify: `server/src/api/tickets.rs`
- Modify: `server/src/api/mod.rs`

- [ ] **Step 1: Add ticket run routes**

In `tickets.rs`:

```rust
// POST /api/tickets/:id/run-agent
async fn run_agent(State(state): State<Arc<AppState>>, Path(ticket_id): Path<Uuid>) -> Result<(StatusCode, Json<RunResponse>), ApiError>

// GET /api/tickets/:id/runs
async fn list_runs(...) -> Result<Json<RunsListResponse>, ApiError>
```

Map `RunError::ActiveRunExists` → 409, validation → 400.

JSON DTO uses camelCase (`ticketId`, `worktreePath`, …) via `#[serde(rename_all = "camelCase")]`.

- [ ] **Step 2: Add agent_runs routes**

```rust
// GET /api/agent-runs/:id
// POST /api/agent-runs/:id/stop
// POST /api/agent-runs/:id/retry
```

- [ ] **Step 3: Add admin jobs route**

`jobs.rs` — `GET /api/agent-jobs` protected by `AdminUser` extractor (same as users routes).

- [ ] **Step 4: Wire in `api/mod.rs`**

```rust
.merge(agent_runs::routes())
.merge(jobs::routes())
```

- [ ] **Step 5: Run clippy + integration tests**

Run: `cargo clippy --workspace -- -D warnings && cargo test -p coppice-server --test integration_agent_runs`

- [ ] **Step 6: Commit**

```bash
git add server/src/api/
git commit -m "feat(server): add agent run and job API routes"
```

---

### Task 10: MockProvider stdout artifact

**Files:**
- Modify: `server/src/providers/mock.rs`

- [ ] **Step 1: Write test for stdout sidecar**

When env `MOCK_AGENT_STDOUT=1`, after reading JSON, write `{artifacts_dir}/runs/{run_id}/stdout.log` with a few lines. Worker passes artifacts dir from config.

- [ ] **Step 2: Implement optional stdout write**

Minimal — 2–3 lines of mock terminal output. No DB artifact row required in M03 (path on run is enough for M04 prep); document in code comment.

- [ ] **Step 3: Commit**

```bash
git add server/src/providers/mock.rs
git commit -m "feat(server): mock provider optional stdout artifact for M04 prep"
```

---

### Task 11: Docker + Compose

**Files:**
- Modify: `deploy/Dockerfile.server`
- Modify: `deploy/docker-compose.yml`
- Modify: `deploy/docker-compose.local.yml`

- [ ] **Step 1: Install git in Dockerfile**

```dockerfile
RUN apt-get update && apt-get install -y ca-certificates git && rm -rf /var/lib/apt/lists/*
```

- [ ] **Step 2: Add volumes and env to both compose files**

Per spec — `worktree_data`, `repo_data`, agent env vars on `server` service.

- [ ] **Step 3: Rebuild and smoke**

Run: `make compose-local-up && make migrate-local && make bootstrap-local`

Expected: server starts, health OK.

- [ ] **Step 4: Commit**

```bash
git add deploy/
git commit -m "chore(deploy): add git, repo and worktree volumes for M03"
```

---

### Task 12: Web — schemas, hooks, Runs tab, drawer header

**Files:**
- Create: `web/src/lib/schemas/agentRun.ts`
- Create: `web/src/features/tickets/useAgentRuns.ts`
- Create: `web/src/features/tickets/TicketRunsTab.tsx`
- Modify: `web/src/features/tickets/TicketDrawer.tsx`
- Modify: `web/src/features/tickets/TicketMetadataTab.tsx`

- [ ] **Step 1: Add Zod schema**

```typescript
// web/src/lib/schemas/agentRun.ts
import { z } from 'zod';

export const runStatusSchema = z.enum([
  'queued', 'running', 'succeeded', 'failed', 'blocked', 'cancelled',
]);

export const agentRunSchema = z.object({
  id: z.string().uuid(),
  ticketId: z.string().uuid(),
  agentId: z.string().uuid(),
  jobType: z.string(),
  status: runStatusSchema,
  sandboxProfileId: z.string(),
  worktreePath: z.string().nullable(),
  branchName: z.string().nullable(),
  startedAt: z.string().nullable(),
  endedAt: z.string().nullable(),
  createdAt: z.string(),
  errorMessage: z.string().nullable(),
});
```

- [ ] **Step 2: Add hooks**

`useAgentRuns(ticketId)` — `useQuery` with `refetchInterval` 3000 when any run is queued/running.

Mutations: `useRunAgent`, `useStopRun`, `useRetryRun` — invalidate runs + ticket + comments on success.

- [ ] **Step 3: Implement `TicketRunsTab`**

Read-only list with status pills (reuse design tokens / Pill-style classes from board). Show agent name via agents query lookup.

- [ ] **Step 4: Update `TicketDrawer`**

- Add `'runs'` to `DrawerTab` and tab labels (order: Description, Comments, Runs, Metadata).
- Header: **Run Agent** + **Stop** buttons (layout A).
- Disable Run Agent when `!ticket.assigneeAgentId || !ticket.repoId`.
- Stop visible when runs query has active run for assignee.

- [ ] **Step 5: Update Metadata tab**

Display `ticket.branchName` and latest run `worktreePath` if available.

- [ ] **Step 6: Run web tests**

Run: `make web-test`

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add web/src/
git commit -m "feat(web): add Runs tab and Run Agent header actions"
```

---

### Task 13: E2E smoke + CI

**Files:**
- Create: `e2e/smoke/m03-agent-run.mjs`
- Modify: `Makefile`
- Modify: `.github/workflows/ci.yml` (if smoke not already chained)

- [ ] **Step 1: Write smoke script**

Follow `e2e/smoke/m02-board.mjs` patterns:

1. Wait for health
2. Bootstrap/login
3. Create project + repo (with note: integration uses API; for smoke use a public tiny repo OR document that smoke creates temp repo via shell helper — prefer API-only: create repo with `remote_url` pointing to `https://github.com/octocat/Hello-World.git` or skip clone in smoke by using file URL created in setup script)

**Pragmatic approach for CI smoke:** Use API to create repo with `remote_url: https://github.com/octocat/Hello-World.git` (network allowed in CI). Assign agent, create ticket, POST run-agent, poll runs until `succeeded`, GET comments, assert body contains mock summary.

4. Assert run list length ≥ 1

- [ ] **Step 2: Extend Makefile**

```makefile
e2e-smoke-m03: compose-up
	node e2e/smoke/m03-agent-run.mjs
```

Optionally chain both in `e2e-smoke`.

- [ ] **Step 3: Run smoke locally**

Run: `make e2e-smoke-m03`

Expected: exit 0

- [ ] **Step 4: Commit**

```bash
git add e2e/smoke/m03-agent-run.mjs Makefile .github/workflows/ci.yml
git commit -m "test(e2e): add M03 agent run smoke script"
```

---

### Task 14: Docs + final verification

**Files:**
- Modify: `AGENTS.md`
- Modify: `docs/architecture.md`
- Modify: `docs/superpowers/specs/2026-06-08-m03-agent-execution-design.md` (status → Approved)

- [ ] **Step 1: Update AGENTS.md**

Change status line to M03 in progress / complete; mention `make e2e-smoke-m03`, agent env vars.

- [ ] **Step 2: Update architecture.md**

Document `workers/`, run pipeline, new tables.

- [ ] **Step 3: Full CI checklist**

Run:

```bash
make migrate
cargo clippy --workspace -- -D warnings
cargo test --workspace
make web-test
make e2e-smoke-m03
```

Expected: all pass

- [ ] **Step 4: Commit**

```bash
git add AGENTS.md docs/
git commit -m "docs: update for M03 agent execution"
```

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| Postgres `agent_jobs` queue + worker | 1, 6, 7 |
| `work_on_ticket` job type | 6, 7 |
| Git worktree per (ticket, agent) | 2, 7 |
| Run lifecycle statuses | 1, 6, 7 |
| Context package `.agent/context.md` | 3, 7 |
| Result contract parser | 4, 7 |
| Agent comments from output | 4, 5, 6, 7 |
| Runs tab + Run/Stop UI (layout A) | 12 |
| MockProvider default | 7, 10 |
| Permissive sandbox placeholder | 5 |
| Lazy clone | 2, 7, 8 |
| Stop + retry API/UI | 6, 9, 12 |
| Parallel different agents | 8 |
| Duplicate same-agent reject | 6, 8 |
| Hybrid blocked handling | 4, 7 |
| CI temp git repo (integration) | 8 |
| Compose volumes + git in image | 11 |
| E2E smoke | 13 |

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-08-m03-agent-execution.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks, fast iteration
2. **Inline Execution** — implement tasks in this session with checkpoints for review

Which approach?
