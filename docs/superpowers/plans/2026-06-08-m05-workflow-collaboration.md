# M05 Workflow & Collaboration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver strict status-gate orchestration, `@mention` clarification/resume, per-status `auto_assign` (default true, backlog false), human Final Approve, and a MockProvider-driven CI smoke path that uses the same worker pipeline as OpenCode.

**Architecture:** After each run, a post-run orchestrator parses the result contract (comment/substatus only — not `nextStatus`), calls `WorkflowService::resolve_transition` for gate moves and assignee/recommendation, then `MentionService` for jobs, then enqueues follow-up work when `auto_start_runs` is enabled. Transition rules live in Rust, not YAML.

**Tech Stack:** Rust (Axum, SQLx, Tokio), React/Vite/TanStack Query, agent-browser E2E, MockProvider fixtures

**Design spec:** [docs/superpowers/specs/2026-06-08-m05-workflow-collaboration-design.md](../specs/2026-06-08-m05-workflow-collaboration-design.md)

---

## File map

| Path | Responsibility |
|------|----------------|
| `server/migrations/009_workflow_collaboration.sql` | `ticket_mentions`, ticket workflow columns |
| `config/src/lib.rs` | `WorkflowConfig`, `auto_assign` per-status map |
| `server/src/domain/workflow.rs` | `TransitionContext`, `TransitionAction`, `PendingRecommendation` |
| `server/src/domain/mention.rs` | `Mention`, `MentionStatus` |
| `server/src/services/workflow_service.rs` | Gate validator + transition resolver |
| `server/src/services/mention_service.rs` | Parse `@agent`, persist mentions, enqueue jobs |
| `server/src/services/run_orchestrator.rs` | Post-run pipeline wiring |
| `server/src/services/result_contract.rs` | Parse contract only; no board status from `nextStatus` |
| `server/src/providers/mod.rs` | Add optional `assignTo` on `AgentRunResult` |
| `server/src/providers/mock.rs` | Role/job fixture resolution |
| `server/src/workers/job_worker.rs` | Run-start transitions, `respond_to_mention`, resume context |
| `server/src/services/run_service.rs` | `enqueue_run_if_ready`, refactor `finish_with_apply` caller |
| `server/src/services/context_builder.rs` | `assignTo` in contract docs; `## Resume` section |
| `server/src/api/tickets.rs` | `final-approve`, `resolve-blocker` |
| `server/src/api/mentions.rs` | `POST /api/mentions/:id/ignore` |
| `server/src/api/comments.rs` | Human `@mention` → MentionService on create |
| `server/src/events/bus.rs` | `agent.mentioned` event |
| `fixtures/agent-responses/{key}/{job-type}.json` | Scope B mock fixtures |
| `web/src/features/tickets/TicketDrawer.tsx` | Final Approve button |
| `web/src/features/tickets/TicketMetadataPanel.tsx` | Pending recommendation badge |
| `web/src/lib/schemas/ticket.ts` | New ticket fields |
| `e2e/smoke/m05-workflow.mjs` | CI smoke scope B |
| `deploy/docker-compose.yml` | `WORKFLOW_AUTO_START_RUNS=true` for server in CI profile |

---

### Task 1: Database migration

**Files:**
- Create: `server/migrations/009_workflow_collaboration.sql`

- [ ] **Step 1: Add migration**

```sql
CREATE TABLE ticket_mentions (
    id UUID PRIMARY KEY,
    ticket_id UUID NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    comment_id UUID NOT NULL REFERENCES ticket_comments(id) ON DELETE CASCADE,
    mentioned_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    resume_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    handled_at TIMESTAMPTZ
);

CREATE INDEX ticket_mentions_ticket_status_idx
    ON ticket_mentions (ticket_id, status);

ALTER TABLE tickets
    ADD COLUMN pending_assign_recommendation JSONB,
    ADD COLUMN clarification_round INT NOT NULL DEFAULT 0;
```

- [ ] **Step 2: Run migration**

Run: `cargo run -p coppice-cli -- migrate`

Expected: migration `009_workflow_collaboration` applied

- [ ] **Step 3: Commit**

```bash
git add server/migrations/009_workflow_collaboration.sql
git commit -m "feat(server): add workflow collaboration tables and ticket columns"
```

---

### Task 2: Workflow config

**Files:**
- Modify: `config/src/lib.rs`
- Modify: `config/src/lib.rs` (tests section at bottom)
- Modify: `config.toml` (example defaults)

- [ ] **Step 1: Write failing test**

Add to `config/src/lib.rs` `#[cfg(test)]`:

```rust
#[test]
fn workflow_auto_assign_backlog_override() {
    let raw = r#"
        [workflow]
        auto_start_runs = false

        [workflow.auto_assign]
        default = true
        backlog = false
    "#;
    let cfg: AppConfig = toml::from_str(&format!(
        "{raw}\n[server]\nport=8080\n[database]\nurl=postgres://x\n[auth]\nsession_secret=s\nbootstrap_password=p\ncookie_secure=false\n[storage]\nartifacts_dir=/tmp\nmax_upload_bytes=1\n[agent]\ndefault_connector=mock\nworktrees_path=/tmp\nworker_count=1\n[web]\nport=5173\nstatic_dir=./web/dist"
    )).expect("parse");
    assert!(!cfg.workflow.auto_assign.effective("backlog"));
    assert!(cfg.workflow.auto_assign.effective("ready"));
    assert!(cfg.workflow.auto_assign.effective("in_progress"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p coppice-config workflow_auto_assign_backlog_override -- --nocapture`

Expected: FAIL — `workflow` field missing on `AppConfig`

- [ ] **Step 3: Add WorkflowConfig**

In `config/src/lib.rs`:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub storage: StorageConfig,
    pub agent: AgentConfig,
    pub web: WebConfig,
    #[serde(default)]
    pub workflow: WorkflowConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowConfig {
    #[serde(default)]
    pub auto_start_runs: bool,
    #[serde(default)]
    pub auto_assign: AutoAssignConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AutoAssignConfig {
    #[serde(default = "default_true")]
    pub default: bool,
    #[serde(default)]
    pub backlog: Option<bool>,
    #[serde(default)]
    pub ready: Option<bool>,
    #[serde(default)]
    pub in_progress: Option<bool>,
    #[serde(default)]
    pub in_review: Option<bool>,
    #[serde(default)]
    pub in_qa: Option<bool>,
    #[serde(default)]
    pub wait_for_final_review: Option<bool>,
    #[serde(default)]
    pub blocked: Option<bool>,
    #[serde(default)]
    pub done: Option<bool>,
}

fn default_true() -> bool {
    true
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            auto_start_runs: false,
            auto_assign: AutoAssignConfig {
                default: true,
                backlog: Some(false),
                ready: None,
                in_progress: None,
                in_review: None,
                in_qa: None,
                wait_for_final_review: None,
                blocked: None,
                done: None,
            },
        }
    }
}

impl AutoAssignConfig {
    pub fn effective(&self, status: &str) -> bool {
        let override_val = match status {
            "backlog" => self.backlog,
            "ready" => self.ready,
            "in_progress" => self.in_progress,
            "in_review" => self.in_review,
            "in_qa" => self.in_qa,
            "wait_for_final_review" => self.wait_for_final_review,
            "blocked" => self.blocked,
            "done" => self.done,
            _ => None,
        };
        override_val.unwrap_or(self.default)
    }
}
```

Add `WorkflowConfig::default()` to `AppConfig` defaults figment `Serialized::defaults`.

Support env override: `WORKFLOW_AUTO_START_RUNS=true` via figment `Env::prefixed("WORKFLOW_")` — extend `apply_env` if not already generic.

- [ ] **Step 4: Update root `config.toml`**

```toml
[workflow]
auto_start_runs = false

[workflow.auto_assign]
default = true
backlog = false
```

- [ ] **Step 5: Run test**

Run: `cargo test -p coppice-config workflow_auto_assign_backlog_override -- --nocapture`

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add config/src/lib.rs config.toml
git commit -m "feat(config): add workflow auto_start_runs and per-status auto_assign"
```

---

### Task 3: Domain types + `assignTo` on result contract

**Files:**
- Create: `server/src/domain/workflow.rs`
- Create: `server/src/domain/mention.rs`
- Modify: `server/src/domain/mod.rs`
- Modify: `server/src/providers/mod.rs`
- Modify: `server/src/domain/ticket.rs`

- [ ] **Step 1: Add domain modules**

`server/src/domain/workflow.rs`:

```rust
use crate::domain::substatus::Substatus;
use crate::domain::ticket::TicketStatus;
use crate::providers::AgentRunResult;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Succeeded,
    Blocked,
}

#[derive(Debug, Clone)]
pub struct TransitionContext {
    pub ticket_id: Uuid,
    pub current_status: TicketStatus,
    pub assignee_agent_id: Option<Uuid>,
    pub agent_role: String,
    pub agent_key: String,
    pub job_type: String,
    pub run_outcome: RunOutcome,
    pub contract: AgentRunResult,
    pub project_agent_keys: Vec<String>,
    pub auto_assign_enabled: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRecommendation {
    pub recommended_agent_key: String,
    pub recommended_by_agent_id: Uuid,
    pub recommended_at: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JobRequest {
    pub job_type: String,
    pub agent_id: Uuid,
    pub resume_agent_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct TransitionAction {
    pub new_status: Option<TicketStatus>,
    pub new_assignee_id: Option<Option<Uuid>>,
    pub pending_recommendation: Option<Option<PendingRecommendation>>,
    pub substatus: Option<Option<Substatus>>,
    pub substatus_metadata: Option<Option<Value>>,
    pub enqueue_jobs: Vec<JobRequest>,
    pub increment_clarification_round: bool,
}
```

`server/src/domain/mention.rs`:

```rust
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MentionStatus {
    Pending,
    Handled,
    Ignored,
}

pub struct TicketMention {
    pub id: Uuid,
    pub ticket_id: Uuid,
    pub comment_id: Uuid,
    pub mentioned_agent_id: Uuid,
    pub resume_agent_id: Option<Uuid>,
    pub status: MentionStatus,
}
```

- [ ] **Step 2: Extend `AgentRunResult`**

In `server/src/providers/mod.rs`, add to both `Done` and `Blocked` variants:

```rust
#[serde(default, rename = "assignTo")]
assign_to: Option<String>,
```

Make `next_status` optional with `#[serde(default)]` on Done/Blocked so agents can omit it:

```rust
#[serde(default, rename = "nextStatus")]
next_status: Option<String>,
```

Update existing fixtures/tests that deserialize contracts.

- [ ] **Step 3: Extend `Ticket` struct**

Add to `server/src/domain/ticket.rs`:

```rust
pub pending_assign_recommendation: Option<serde_json::Value>,
pub clarification_round: i32,
```

Update `TicketService` row mapping and API `TicketResponse` in a follow-up step within this task.

- [ ] **Step 4: Commit**

```bash
git add server/src/domain/workflow.rs server/src/domain/mention.rs server/src/domain/mod.rs server/src/providers/mod.rs server/src/domain/ticket.rs
git commit -m "feat(server): add workflow domain types and assignTo on agent result contract"
```

---

### Task 4: Result contract — parse only, no board status

**Files:**
- Modify: `server/src/services/result_contract.rs`
- Test: same file `#[cfg(test)]`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn apply_does_not_set_ticket_status_from_next_status() {
    let result = AgentRunResult::Done {
        summary: "Done.".into(),
        changed_files: vec![],
        tests_run: vec![],
        next_status: Some("Done".into()),
        mention_agents: vec![],
        blockers: vec![],
        assign_to: None,
    };
    let applied = apply_agent_result(&result).expect("apply");
    assert_eq!(applied.run_status, RunStatus::Succeeded);
    assert!(applied.ticket.status.is_none());
    assert_eq!(applied.comment.body, "Done.");
}
```

Adjust `ApplyTicketUpdate` to use `status: Option<TicketStatus>` (and same for substatus fields where workflow will set them).

- [ ] **Step 2: Run test — expect FAIL**

Run: `cargo test -p coppice-server apply_does_not_set_ticket_status_from_next_status -- --nocapture`

- [ ] **Step 3: Refactor `apply_agent_result`**

Remove `ticket_status_from_next_status` usage from `apply_agent_result`. Return:

```rust
pub struct ApplyTicketUpdate {
    pub status: Option<TicketStatus>,
    pub substatus: Option<Substatus>,
    pub substatus_metadata: Option<serde_json::Value>,
}
```

For `blocked`, still compute substatus metadata from `blockerType` but leave `status: None` — workflow sets `Blocked` when appropriate.

Update `done_fixture_maps_to_succeeded_in_review` test to expect `status: None`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p coppice-server result_contract -- --nocapture`

Expected: PASS (update old tests inline)

- [ ] **Step 5: Commit**

```bash
git add server/src/services/result_contract.rs
git commit -m "refactor(server): result contract no longer applies nextStatus to board"
```

---

### Task 5: WorkflowService — gates and transitions

**Files:**
- Create: `server/src/services/workflow_service.rs`
- Modify: `server/src/services/mod.rs`
- Test: `server/src/services/workflow_service.rs`

- [ ] **Step 1: Gate validator tests**

```rust
#[test]
fn rejects_backlog_to_done() {
    assert!(!WorkflowService::is_legal_transition(
        TicketStatus::Backlog,
        TicketStatus::Done,
    ));
}

#[test]
fn case1_pm_backlog_to_ready_with_pending_recommendation() {
    let action = WorkflowService::resolve_transition(TransitionContext {
        current_status: TicketStatus::Backlog,
        agent_role: "PM".into(),
        agent_key: "pm".into(),
        job_type: "work_on_ticket".into(),
        run_outcome: RunOutcome::Succeeded,
        auto_assign_enabled: false,
        contract: done_with_assign_to("engineer"),
        project_agent_keys: vec!["pm".into(), "backend_engineer".into()],
        ..minimal_ctx()
    }).expect("resolve");
    assert_eq!(action.new_status, Some(TicketStatus::Ready));
    assert!(action.pending_recommendation.unwrap().is_some());
    assert!(action.new_assignee_id.is_none());
}

#[test]
fn case4_missing_assign_to_agent_blocks() {
    let action = WorkflowService::resolve_transition(TransitionContext {
        auto_assign_enabled: true,
        contract: done_with_assign_to("frontend_engineer"),
        project_agent_keys: vec!["pm".into()],
        ..minimal_ctx()
    }).expect("resolve");
    assert_eq!(action.new_status, Some(TicketStatus::Blocked));
}

#[test]
fn respond_to_mention_does_not_change_status() {
    let action = WorkflowService::resolve_transition(TransitionContext {
        job_type: "respond_to_mention".into(),
        run_outcome: RunOutcome::Succeeded,
        ..minimal_ctx()
    }).expect("resolve");
    assert!(action.new_status.is_none());
}
```

- [ ] **Step 2: Run tests — FAIL**

Run: `cargo test -p coppice-server workflow_service -- --nocapture`

- [ ] **Step 3: Implement `WorkflowService`**

Key helpers:

```rust
pub struct WorkflowService;

impl WorkflowService {
    pub fn is_legal_transition(from: TicketStatus, to: TicketStatus) -> bool {
        use TicketStatus::*;
        matches!(
            (from, to),
            (Backlog, Ready)
                | (Backlog, InProgress)
                | (Backlog, Blocked)
                | (Ready, InProgress)
                | (Ready, Blocked)
                | (InProgress, InReview)
                | (InProgress, Blocked)
                | (InReview, InQa)
                | (InReview, Blocked)
                | (InQa, WaitForFinalReview)
                | (InQa, Blocked)
                | (Blocked, Ready)
                | (Blocked, InProgress)
                | (Blocked, Backlog)
                | (WaitForFinalReview, Done) // only via final_approve API — not from agent path
        )
    }

    pub fn resolve_transition(ctx: TransitionContext) -> Result<TransitionAction, String> {
        // respond_to_mention: no status change
        if ctx.job_type == "respond_to_mention" && ctx.run_outcome == RunOutcome::Succeeded {
            return Ok(TransitionAction::default());
        }
        // blocked + mentionAgents: substatus waiting_for_agent, enqueue respond_to_mention via orchestrator
        // implementer roles: researcher, engineer, frontend_engineer, backend_engineer
        // PM on backlog succeeded → Ready + assignTo handling
        // engineer backlog succeeded (case 2 direct) → InReview
        // in_progress implementer succeeded → InReview
        // scope B shortcut: in_review implementer succeeded → WaitForFinalReview + unassign (skip QA in smoke)
        // assignTo resolution via project_agent_keys; missing → Blocked
    }

    pub fn resolve_run_start_transition(
        current: TicketStatus,
        agent_role: &str,
        job_type: &str,
    ) -> Option<TicketStatus> {
        if job_type != "work_on_ticket" {
            return None;
        }
        match (current, agent_role) {
            (TicketStatus::Backlog, role) if is_implementer(role) => Some(TicketStatus::InProgress),
            (TicketStatus::Ready, role) if is_implementer(role) => Some(TicketStatus::InProgress),
            _ => None,
        }
    }
}
```

`is_implementer` matches role strings case-insensitively: `Engineer`, `Frontend Engineer`, `Backend Engineer`, `Research`, etc.

`final_approve` is a separate method:

```rust
pub fn final_approve(current: TicketStatus) -> Result<TicketStatus, &'static str> {
    if current == TicketStatus::WaitForFinalReview {
        Ok(TicketStatus::Done)
    } else {
        Err("final approve requires wait_for_final_review")
    }
}
```

- [ ] **Step 4: Run tests — PASS**

- [ ] **Step 5: Commit**

```bash
git add server/src/services/workflow_service.rs server/src/services/mod.rs
git commit -m "feat(server): add WorkflowService status gates and transition resolver"
```

---

### Task 6: MentionService

**Files:**
- Create: `server/src/services/mention_service.rs`
- Modify: `server/src/services/mod.rs`
- Test: `server/src/services/mention_service.rs`

- [ ] **Step 1: Mention parser test**

```rust
#[test]
fn parses_agent_keys_from_comment_body() {
    let keys = MentionService::parse_mention_keys(
        "@backend_engineer thoughts on option A vs B?",
        &["pm", "backend_engineer"],
    );
    assert_eq!(keys, vec!["backend_engineer"]);
}
```

- [ ] **Step 2: Implement MentionService**

```rust
pub struct MentionService<'a> {
    pool: &'a PgPool,
}

impl<'a> MentionService<'a> {
    pub fn parse_mention_keys(body: &str, known_keys: &[String]) -> Vec<String> {
        let mut found = Vec::new();
        for key in known_keys {
            let needle = format!("@{key}");
            if body.contains(&needle) && !found.contains(key) {
                found.push(key.clone());
            }
        }
        found
    }

    pub async fn create_mentions(
        &self,
        ticket_id: Uuid,
        comment_id: Uuid,
        keys: &[String],
        resume_agent_id: Option<Uuid>,
        project_id: Uuid,
    ) -> Result<Vec<TicketMention>, MentionError> {
        // resolve keys → agent ids via AgentService::list_for_project + preset_source match
        // INSERT ticket_mentions
    }

    pub async fn mark_handled(&self, mention_id: Uuid) -> Result<(), MentionError> { /* ... */ }
}
```

Agent key resolution: match `agents.preset_source` to key; fallback `slugify(agent.name)`.

- [ ] **Step 3: Commit**

```bash
git add server/src/services/mention_service.rs server/src/services/mod.rs
git commit -m "feat(server): add MentionService for comment and contract mentions"
```

---

### Task 7: Post-run orchestrator

**Files:**
- Create: `server/src/services/run_orchestrator.rs`
- Modify: `server/src/services/run_service.rs`
- Modify: `server/src/workers/job_worker.rs`

- [ ] **Step 1: Integration-style unit test with test DB** (or extract pure `build_post_run_plan` tested without DB)

Test that orchestrator:
- Applies workflow status not contract `nextStatus`
- Creates pending recommendation when `auto_assign` false at backlog
- Enqueues `respond_to_mention` when `mentionAgents` present on blocked run

- [ ] **Step 2: Implement `RunOrchestrator::finish_run`**

```rust
pub struct RunOrchestrator<'a> {
    pool: &'a PgPool,
    workflow: &'a WorkflowConfig,
}

impl<'a> RunOrchestrator<'a> {
    pub async fn finish_run(
        &self,
        run: &AgentRun,
        apply: ApplyResult,
        worktree_path: Option<String>,
        branch_name: Option<String>,
    ) -> Result<AgentRun, RunError> {
        // 1. Load ticket, agent, project agents
        // 2. Build TransitionContext from apply + run
        // 3. action = WorkflowService::resolve_transition(...)
        // 4. Merge apply.ticket substatus with action substatus
        // 5. TicketService::update_status_and_workflow_fields(...)
        // 6. CommentService::create(...)
        // 7. MentionService::create_mentions from contract mentionAgents + resume_agent_id
        // 8. For each JobRequest + mention jobs: JobService::enqueue → RunService::start_run if auto_start_runs && assignee set
        // 9. Update agent_runs row status
    }
}
```

Refactor `job_worker` to call `RunOrchestrator::finish_run` instead of `finish_with_apply`.

`TicketService` new method:

```rust
pub async fn apply_workflow_update(
    &self,
    ticket_id: Uuid,
    status: Option<TicketStatus>,
    substatus: Option<Option<Substatus>>,
    substatus_metadata: Option<Option<Value>>,
    assignee: Option<Option<Uuid>>,
    pending_recommendation: Option<Option<PendingRecommendation>>,
    clarification_round_delta: i32,
) -> Result<TicketWithDisplay, TicketError>
```

- [ ] **Step 3: Run server tests**

Run: `cargo test -p coppice-server -- --nocapture`

- [ ] **Step 4: Commit**

```bash
git add server/src/services/run_orchestrator.rs server/src/services/run_service.rs server/src/services/ticket_service.rs server/src/workers/job_worker.rs
git commit -m "feat(server): post-run orchestrator wires workflow and mentions"
```

---

### Task 8: Run-start transitions + job types in worker

**Files:**
- Modify: `server/src/workers/job_worker.rs`
- Modify: `server/src/providers/mod.rs` (`AgentRunInput`)
- Modify: `server/src/services/context_builder.rs`

- [ ] **Step 1: Extend `AgentRunInput`**

```rust
pub struct AgentRunInput {
    // existing fields...
    pub agent_key: String,
    pub job_type: String,
    pub resume_context: Option<String>,
}
```

- [ ] **Step 2: Apply run-start gate in `mark_running` hook**

After `mark_running`, call `WorkflowService::resolve_run_start_transition` and update ticket status if `Some`.

- [ ] **Step 3: Resume context in context builder**

When `resume_context` is set, append to generated `context.md`:

```markdown
## Resume

{resume_context}
```

Build `resume_context` in orchestrator when enqueueing resume job: include blocker comment + PM answer.

- [ ] **Step 4: Pass fields from worker**

```rust
let agent_key = agent.preset_source.clone().unwrap_or_else(|| slugify(&agent.name));
connector.run(AgentRunInput {
    agent_key,
    job_type: run.job_type.clone(),
    resume_context: load_resume_context(pool, run).await?,
    // ...
})
```

- [ ] **Step 5: Commit**

```bash
git add server/src/workers/job_worker.rs server/src/providers/mod.rs server/src/services/context_builder.rs
git commit -m "feat(server): run-start gate transitions and resume context in worker"
```

---

### Task 9: Auto-start enqueue on assign + clarification resume

**Files:**
- Modify: `server/src/api/tickets.rs` (`assign_agent` handler)
- Modify: `server/src/services/run_service.rs`

- [ ] **Step 1: `assign_agent` clears recommendation and optionally auto-starts**

```rust
// After TicketService::assign_agent:
if let Some(rec) = &ticket.pending_assign_recommendation {
    let _ = ticket_svc.clear_pending_recommendation(ticket_id).await?;
}
if state.config.workflow.auto_start_runs {
    if ticket.assignee_agent_id.is_some() && ticket.repo_id.is_some() {
        let _ = RunService::new(pool).start_run(ticket_id).await;
    }
}
```

`start_run` already rejects when no assignee — keep that guard.

- [ ] **Step 2: Clarification resume in orchestrator**

When PM `respond_to_mention` succeeds and mention has `resume_agent_id`:
- `mark_handled`
- clear `waiting_for_agent` substatus
- if `clarification_round < MAX_CLARIFICATION_ROUNDS`: assign resume agent + enqueue `work_on_ticket`
- else: `waiting_for_human` + system comment

Constants in `workflow_service.rs` or `mention_service.rs`:

```rust
pub const MAX_CLARIFICATION_ROUNDS: u32 = 3;
pub const MAX_MENTIONS_PER_RUN: u32 = 2;
```

- [ ] **Step 3: Commit**

```bash
git add server/src/api/tickets.rs server/src/services/run_service.rs
git commit -m "feat(server): auto-start on assign and clarification resume enqueue"
```

---

### Task 10: Human comment mentions + APIs

**Files:**
- Modify: `server/src/api/comments.rs`
- Modify: `server/src/api/tickets.rs`
- Create: `server/src/api/mentions.rs`
- Modify: `server/src/api/mod.rs`
- Modify: `server/src/events/bus.rs`

- [ ] **Step 1: Human comment → mentions (case 3)**

In `create_comment` after insert:

```rust
if author_type == AuthorType::Human {
    let keys = MentionService::parse_mention_keys(&body, &project_agent_keys);
    if !keys.is_empty() {
        let mentions = mention_svc.create_mentions(ticket_id, comment.id, &keys, None, project_id).await?;
        for m in mentions {
            enqueue respond_to_mention job for m.mentioned_agent_id
            publish AppEvent::AgentMentioned { ... }
        }
    }
}
```

Add `AppEvent::AgentMentioned` variant.

- [ ] **Step 2: Final approve endpoint**

```rust
// POST /api/tickets/:id/final-approve
let next = WorkflowService::final_approve(ticket.status)?;
ticket_svc.update_status(ticket_id, next, None, None).await?;
```

- [ ] **Step 3: Resolve blocker + ignore mention**

`POST /api/tickets/:id/resolve-blocker` — clears blocked substatus; optional status restore from metadata or human picks via existing patch status.

`POST /api/mentions/:id/ignore` — sets mention ignored, substatus `waiting_for_human`.

- [ ] **Step 4: Extend ticket API response**

```rust
pending_assign_recommendation: Option<PendingRecommendation>,
clarification_round: i32,
```

- [ ] **Step 5: Integration tests in `server/tests/integration_workflow.rs`**

- [ ] **Step 6: Commit**

```bash
git add server/src/api/comments.rs server/src/api/tickets.rs server/src/api/mentions.rs server/src/api/mod.rs server/src/events/bus.rs server/tests/integration_workflow.rs
git commit -m "feat(server): final-approve, mention APIs, human @mention jobs"
```

---

### Task 11: MockProvider role/job fixtures

**Files:**
- Modify: `server/src/providers/mock.rs`
- Create: `fixtures/agent-responses/pm/work_on_ticket.json`
- Create: `fixtures/agent-responses/backend_engineer/work_on_ticket.json`
- Create: `fixtures/agent-responses/pm/respond_to_mention.json`
- Create: `fixtures/agent-responses/backend_engineer/resume.json`

- [ ] **Step 1: PM fixture**

`fixtures/agent-responses/pm/work_on_ticket.json`:

```json
{
  "status": "done",
  "summary": "Ticket enriched with acceptance criteria.",
  "assignTo": "backend_engineer",
  "changedFiles": [],
  "testsRun": [],
  "mentionAgents": [],
  "blockers": []
}
```

- [ ] **Step 2: Engineer blocked fixture**

`fixtures/agent-responses/backend_engineer/work_on_ticket.json`:

```json
{
  "status": "blocked",
  "blockerType": "needs_human",
  "summary": "@pm Should we use option A or B for the API shape?",
  "mentionAgents": ["pm"],
  "requiredCapabilities": [],
  "requiredSecrets": []
}
```

- [ ] **Step 3: PM respond + engineer resume fixtures**

`pm/respond_to_mention.json` — done, summary with answer.  
`backend_engineer/resume.json` — done, summary implementation complete (workflow moves to Wait for Final Review per scope B).

- [ ] **Step 4: Update `MockProvider::run`**

```rust
fn fixture_path(&self, input: &AgentRunInput) -> PathBuf {
    if let Ok(override_name) = std::env::var("MOCK_AGENT_RESPONSE") {
        return self.fixtures_dir.join(format!("{override_name}.json"));
    }
    let keyed = self.fixtures_dir
        .join(&input.agent_key)
        .join(format!("{}.json", input.job_type));
    if keyed.exists() {
        return keyed;
    }
    // resume alias: second work_on_ticket on same agent may use resume.json
    let resume = self.fixtures_dir.join(&input.agent_key).join("resume.json");
    if input.job_type == "work_on_ticket" && resume.exists() && input.resume_context.is_some() {
        return resume;
    }
    self.fixtures_dir.join(&input.agent_key).join("default.json")
}
```

- [ ] **Step 5: Mock provider test**

```rust
#[tokio::test]
async fn resolves_fixture_by_agent_key_and_job_type() {
    let provider = MockProvider::new(fixtures_root());
    let result = provider.run(AgentRunInput {
        agent_key: "pm".into(),
        job_type: "work_on_ticket".into(),
        // ...
    }).await.expect("run");
    // assert assignTo in done variant
}
```

- [ ] **Step 6: Commit**

```bash
git add server/src/providers/mock.rs fixtures/agent-responses/
git commit -m "feat(server): MockProvider resolves fixtures by agent key and job type"
```

---

### Task 12: Agent context templates

**Files:**
- Modify: `server/src/services/context_builder.rs`
- Modify: `server/agent_templates/pm.md` (contract section reference only if duplicated in template)

- [ ] **Step 1: Update expected output contract in `context_builder.rs`**

Replace `nextStatus` emphasis with:

```markdown
- `assignTo`: agent key to recommend as next assignee (e.g. `backend_engineer`, `research`)
- Server ignores `nextStatus` for board moves; workflow gates control columns
```

- [ ] **Step 2: Commit**

```bash
git add server/src/services/context_builder.rs
git commit -m "docs(server): context contract documents assignTo instead of nextStatus"
```

---

### Task 13: Frontend — recommendation, Final Approve, substatus

**Files:**
- Modify: `web/src/lib/schemas/ticket.ts`
- Modify: `web/src/features/tickets/TicketMetadataPanel.tsx`
- Modify: `web/src/features/tickets/TicketDrawer.tsx`
- Modify: `web/src/features/tickets/useTicket.ts`
- Test: `web/src/features/tickets/TicketDrawer.test.tsx`

- [ ] **Step 1: Extend ticket schema**

```typescript
export const pendingRecommendationSchema = z.object({
  recommendedAgentKey: z.string(),
  recommendedByAgentId: z.string().uuid(),
  recommendedAt: z.string(),
  summary: z.string().optional(),
});

// In ticketSchema:
pendingAssignRecommendation: pendingRecommendationSchema.nullable().optional(),
clarificationRound: z.number().optional(),
```

- [ ] **Step 2: Recommendation badge in metadata panel**

When `ticket.pendingAssignRecommendation` present:

```tsx
<p className="text-sm text-muted-foreground">
  Recommends: <span className="font-medium">{rec.recommendedAgentKey}</span>
</p>
```

- [ ] **Step 3: Final Approve button in drawer header**

When `ticket.status === 'wait_for_final_review'`:

```tsx
<Button onClick={() => finalApprove.mutate(ticket.id)}>Final Approve</Button>
```

Add `useFinalApprove` mutation → `POST /api/tickets/:id/final-approve`.

- [ ] **Step 4: Vitest**

Assert Final Approve button renders only in `wait_for_final_review`.

- [ ] **Step 5: Run web tests**

Run: `cd web && yarn test`

- [ ] **Step 6: Commit**

```bash
git add web/src/lib/schemas/ticket.ts web/src/features/tickets/
git commit -m "feat(web): pending recommendation badge and Final Approve action"
```

---

### Task 14: Integration test — scope B mock pipeline

**Files:**
- Create: `server/tests/integration_workflow.rs`
- Modify: `server/tests/common/mod.rs` (helpers for PM + backend_engineer agents)

- [ ] **Step 1: Write integration test**

```rust
#[tokio::test]
async fn scope_b_mock_pipeline_reaches_final_review() {
    let app = setup_test_app_with_workflow(WorkflowConfig {
        auto_start_runs: true,
        ..WorkflowConfig::default()
    }).await;
    // create project, repo, pm agent (preset pm, connector mock), backend_engineer agent
    // create ticket backlog, assign repo, assign pm
    // wait for PM run → ticket status ready + pending_assign_recommendation
    // assign backend_engineer (clears recommendation)
    // wait for engineer blocked + mention
    // wait for pm respond_to_mention
    // wait for engineer resume → wait_for_final_review
    // POST final-approve → done
}
```

Use existing test harness patterns from `integration_workspace.rs` / agent run tests.

- [ ] **Step 2: Run test**

Run: `cargo test -p coppice-server scope_b_mock_pipeline -- --nocapture`

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add server/tests/integration_workflow.rs server/tests/common/mod.rs
git commit -m "test(server): integration test for M05 scope B mock pipeline"
```

---

### Task 15: E2E smoke + deploy delta

**Files:**
- Create: `e2e/smoke/m05-workflow.mjs`
- Modify: `Makefile`
- Modify: `deploy/docker-compose.yml` (or CI env in workflow)
- Modify: `.github/workflows/ci.yml` (if smoke chain exists)

- [ ] **Step 1: E2E script** (follow `m04-live-console.mjs` patterns)

Flow:
1. Login, create project/ticket/repo
2. Create PM + backend_engineer agents with `connector: mock`
3. Assign PM, wait for Ready + recommendation text in ticket GET
4. Assign backend_engineer
5. Poll until status `wait_for_final_review` (timeout ~120s)
6. `POST /api/tickets/:id/final-approve`
7. Assert status `done`

Env: `WORKFLOW_AUTO_START_RUNS=true` on server container.

- [ ] **Step 2: Makefile target**

```makefile
e2e-smoke-m05:
	node e2e/smoke/m05-workflow.mjs
```

- [ ] **Step 3: Run smoke locally**

Run: `make compose-up && make bootstrap && make e2e-smoke-m05`

- [ ] **Step 4: Commit**

```bash
git add e2e/smoke/m05-workflow.mjs Makefile deploy/docker-compose.yml .github/workflows/ci.yml
git commit -m "ci: add M05 workflow collaboration smoke test"
```

---

### Task 16: Docs and milestone acceptance

**Files:**
- Modify: `docs/superpowers/specs/2026-06-08-m05-workflow-collaboration-design.md` (Status → Approved)
- Modify: `AGENTS.md`
- Modify: `docs/milestones/M05-workflow-and-collaboration.md` (check acceptance boxes)
- Modify: `docs/milestones/README.md` if needed

- [ ] **Step 1: Update AGENTS.md status line**

```markdown
**Status:** M05 workflow & collaboration in progress.
**Next:** complete M05 acceptance, then M06.
```

- [ ] **Step 2: Verify full test suite**

Run: `make test && make web-test`

- [ ] **Step 3: Commit**

```bash
git add AGENTS.md docs/
git commit -m "chore(m05): update docs and mark design spec approved"
```

---

## Self-review (spec coverage)

| Spec requirement | Task |
|------------------|------|
| Status gates in code; `nextStatus` ignored | 4, 5, 7 |
| `assignTo` + per-status `auto_assign` | 2, 3, 5, 7 |
| Pending recommendation at backlog | 5, 7, 13 |
| Mentions + clarification/resume | 6, 7, 9, 10 |
| `respond_to_mention` no status move (case 3) | 5, 10 |
| Missing assignTo agent → Blocked (case 4) | 5 |
| Final Approve gate | 5, 10, 13 |
| MockProvider fixtures + CI smoke | 11, 14, 15 |
| `auto_start_runs`; no assignee → no run | 2, 9 |
| Communication limits | 9 (constants) |
| `agent.mentioned` event | 10 |

No placeholder steps. Types consistent: `assignTo` / `assign_to` / `recommendedAgentKey` follow existing serde camelCase API conventions.

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-08-m05-workflow-collaboration.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks, fast iteration  
2. **Inline Execution** — run tasks in this session with executing-plans, batch checkpoints

Which approach do you want?
