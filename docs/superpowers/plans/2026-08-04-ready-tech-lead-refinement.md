# Ready Tech Lead Refinement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a Tech Lead's `work_on_ticket` run in `Ready` a mandatory no-code technical-refinement handoff to an enabled implementer.

**Architecture:** Select the contract and workflow behavior from the existing ticket status plus agent identity. Extend the pure workflow context with enabled implementer keys, reuse `auto_assign` and the orchestrator's assignee auto-start path, keep blocked clarification on existing mention/resume jobs, and add a worker-side no-git guard.

**Tech Stack:** Rust, Axum services, SQLx/PostgreSQL integration tests, Serde JSON, Markdown context generation, MockProvider fixtures, Node M05 smoke script.

---

### Task 1: Ready technical-refinement context

**Files:**
- Modify: `server/src/services/context_builder.rs`

- [ ] **Step 1: Write failing context tests**

Add `tech_lead_in_ready_context_requires_no_code_refinement_handoff` with a `ContextInput` whose status is `ready`, key is `tech_lead`, and role is `Technical Lead`. Assert that the output contains the Ready technical-refinement heading, `updatedDescription`, `acceptanceCriteria`, required `assignTo`, `changedFiles: []`, approach/risks language, and explicit source-edit prohibition. Assert that it omits `Coppice platform rules — git`, `implementer completion`, and code-review guidance.

Extend `pm_context_includes_platform_refinement_rules` to assert that PM guidance names ownership, consultation, and notification; says successful `mentionAgents` is notification-only; and forbids combining fields for the same target.

- [ ] **Step 2: Verify the tests fail**

Run: `cargo test -p coppice-server --lib services::context_builder::tests`

Expected: FAIL because Ready-specific Tech Lead guidance and the PM intent rules do not exist.

- [ ] **Step 3: Implement status-and-role guidance selection**

Add `is_ready_tech_lead_task(input)` beside the existing review/QC selectors. In `format_contract_guidance`, keep PM first, then return a Ready-specific contract before review/QC/generic implementer rules. Require read-only repository inspection, a concrete approach and risks in ticket fields/comment, `changedFiles: []`, a valid enabled implementer `assignTo` on success, and PM mention-only clarification on blocked results. Do not append `format_git_rules()`.

Update PM guidance with these exact semantics:

```text
assignTo      = formal ownership/recommendation
agentRequests = bounded consultation
mentionAgents = notification only after success
one target may appear in only one intent field
```

- [ ] **Step 4: Run focused context tests**

Run: `cargo test -p coppice-server --lib services::context_builder::tests`

Expected: PASS, including unchanged In Review and In QA contracts.

- [ ] **Step 5: Commit**

Run: `git add server/src/services/context_builder.rs && git commit -m "feat(server): add Ready Tech Lead refinement context"`

### Task 2: Pure Ready-stage workflow resolution

**Files:**
- Modify: `server/src/domain/workflow.rs`
- Modify: `server/src/services/workflow_service.rs`

- [ ] **Step 1: Write failing resolver tests**

Add unit cases for:

- Ready Tech Lead run start stays `Ready`, including a role containing both “Technical Lead” and “Engineer”.
- Valid enabled implementer plus `auto_assign = true` changes assignee, clears pending recommendation, leaves status unset, and does not directly enqueue a workflow job.
- Valid enabled implementer plus `auto_assign = false` stores a pending recommendation, leaves assignee/status unchanged, and enqueues nothing.
- Missing, blank, unknown/disabled-unavailable, and enabled non-implementer `assignTo` values leave status/assignment unchanged and return one actionable system comment.
- Blocked Ready Tech Lead mention uses the clarification response job with `resume_agent_id` and does not use the verification handoff.

- [ ] **Step 2: Verify the tests fail**

Run: `cargo test -p coppice-server --lib services::workflow_service::tests::ready_tech_lead`

Expected: FAIL because Ready Tech Leads have no success branch and blocked results are treated as verification handoffs.

- [ ] **Step 3: Add enabled implementer identity to transition context**

Add `project_implementer_keys: Vec<String>` to `TransitionContext`. The orchestrator will populate it from enabled agents whose role satisfies the existing `is_implementer` predicate, preserving preset and name-slug aliases.

- [ ] **Step 4: Implement the refinement branch**

Add a Ready/Tech-Lead predicate using `agent_key == "tech_lead"` or the existing Tech Lead role matcher. After continued-result handling and before generic assignment/gates, resolve successful refinement as follows:

```text
missing/blank assignTo -> actionable system comment, no status/assignee/job
unknown or disabled key -> actionable system comment, no status/assignee/job
known non-implementer -> actionable system comment, no status/assignee/job
enabled implementer -> apply_assign_to according to auto_assign, no status gate
```

List available implementer keys in failure comments. Exclude Ready Tech Leads from `resolve_verification_handoff` so blocked PM clarification follows the generic mention/resume branch. Guard `resolve_run_start_transition` so Ready Tech Leads cannot match the broad engineer predicate.

- [ ] **Step 5: Run workflow tests**

Run: `cargo test -p coppice-server --lib services::workflow_service::tests`

Expected: PASS, including implementer, review, QC, and final-approval regressions.

- [ ] **Step 6: Commit**

Run: `git add server/src/domain/workflow.rs server/src/services/workflow_service.rs && git commit -m "feat(server): resolve Ready Tech Lead handoffs"`

### Task 3: Orchestration and git boundaries

**Files:**
- Modify: `server/src/services/run_orchestrator.rs`
- Modify: `server/src/workers/job_worker.rs`

- [ ] **Step 1: Write failing unit tests**

Add worker tests proving a Ready Tech Lead `work_on_ticket` run does not finalize git while an In Review Tech Lead and a Ready backend implementer retain existing behavior.

Add orchestrator helper tests proving enabled implementer aliases are derived from the same resolved agent IDs as assignment aliases. Extend successful-consultation dispatch coverage with `allow_consultation_dispatch = false`, expecting structured request mentions to be handled without a response job.

- [ ] **Step 2: Verify the tests fail**

Run: `cargo test -p coppice-server --lib workers::job_worker::tests::should_not_finalize_git_for_ready_tech_lead services::run_orchestrator::tests::technical_refinement`

Expected: FAIL because Ready Tech Lead work currently finalizes git and refinement results may dispatch consultations.

- [ ] **Step 3: Populate implementer keys and suppress extra runs**

Extend `build_project_agent_maps` to return enabled implementer aliases after resolving duplicate keys to stable agent IDs. Pass them into `TransitionContext`.

Compute whether the source is a successful Ready-stage Tech Lead refinement from the pre-update status and agent identity. Pass an `allow_consultation_dispatch` flag into `enqueue_successful_consultation_jobs`; when false, mark accepted request mentions handled and enqueue no consultation. The only automatic run after a valid handoff must remain the orchestrator's existing `action.new_assignee_id` `work_on_ticket` start.

- [ ] **Step 4: Skip Ready Tech Lead finalization**

Add a worker Tech Lead identity helper based on preset source or role. Make `should_finalize_worktree_git` return false for `(Tech Lead, Ready)` before the existing QC rule. Leave `should_finalize_run_git` restricted to `work_on_ticket`.

- [ ] **Step 5: Run focused unit tests**

Run: `cargo test -p coppice-server --lib services::run_orchestrator::tests workers::job_worker::tests`

Expected: PASS.

- [ ] **Step 6: Commit**

Run: `git add server/src/services/run_orchestrator.rs server/src/workers/job_worker.rs && git commit -m "fix(server): enforce Ready refinement run boundaries"`

### Task 4: Status-aware MockProvider fixtures

**Files:**
- Modify: `server/src/providers/mock.rs`
- Modify: `fixtures/agent-responses/pm/work_on_ticket.json`
- Create: `fixtures/agent-responses/tech_lead/ready.json`
- Create: `fixtures/agent-responses/tech_lead/work_on_ticket.json`
- Create: `fixtures/agent-responses/tech_lead_clarification/ready.json`
- Create: `fixtures/agent-responses/tech_lead_clarification/resume.json`
- Create: `fixtures/agent-responses/tech_lead_missing/ready.json`
- Create: `fixtures/agent-responses/tech_lead_unknown/ready.json`
- Create: `fixtures/agent-responses/tech_lead_disabled/ready.json`

- [ ] **Step 1: Write a failing provider-routing test**

Write a temporary context file containing `**Status:** ready`, point a Tech Lead `AgentRunInput.context_path` at it, and assert MockProvider returns the Ready fixture with `assignTo: "backend_engineer"`. Point a second context at `in_review` and assert the ordinary work fixture has no assignment.

- [ ] **Step 2: Verify the provider test fails**

Run: `cargo test -p coppice-server --lib providers::mock::tests::tech_lead_ready_uses_refinement_fixture`

Expected: FAIL because the Ready-specific fixture route does not exist.

- [ ] **Step 3: Add status-aware fixture selection**

After resume selection and before the normal per-job path, detect a full `work_on_ticket` context whose markdown ticket status is `ready`; select `<agent_key>/ready.json` when present. Do not infer readiness from arbitrary thread text.

Change the default PM fixture to `assignTo: "tech_lead"` with empty `mentionAgents` and `agentRequests`. Add a Ready Tech Lead fixture that updates the technical approach and risks, leaves `changedFiles` empty, and assigns `backend_engineer`. Keep the ordinary Tech Lead fixture review-only with no assignment. Add focused clarification and invalid-target fixtures for integration cases.

- [ ] **Step 4: Run provider tests**

Run: `cargo test -p coppice-server --lib providers::mock::tests`

Expected: PASS.

- [ ] **Step 5: Commit**

Run: `git add server/src/providers/mock.rs fixtures/agent-responses && git commit -m "test(mock): script Tech Lead refinement handoffs"`

### Task 5: Workflow integration coverage

**Files:**
- Modify: `server/tests/integration_workflow.rs`

- [ ] **Step 1: Update the main mock pipeline**

Create PM, Tech Lead, and backend engineer agents. Assert PM completion leaves the ticket `ready` with a `tech_lead` pending recommendation and no Tech Lead run. Manually assign Tech Lead, assert its run starts without leaving `ready`, then assert successful refinement auto-assigns the backend engineer and creates exactly one backend `work_on_ticket` run. Continue through the existing engineer clarification and final-approval path.

- [ ] **Step 2: Add invalid-handoff integration cases**

Run Ready Tech Lead fixtures that omit `assignTo`, name an unknown key, and name a disabled backend engineer. For each, assert the ticket remains `ready`, no target run exists, and the latest system comment explains how to select an enabled implementer.

- [ ] **Step 3: Add manual Ready auto-assign policy coverage**

Start workers with `config.workflow.auto_assign.ready = Some(false)`, run a valid Ready Tech Lead fixture, and assert the ticket stays assigned to Tech Lead with a pending `backend_engineer` recommendation and no backend run.

- [ ] **Step 4: Add Ready clarification/resume coverage**

Use `tech_lead_clarification`: initial Ready run blocks and mentions PM, PM response succeeds, one resumed Tech Lead `work_on_ticket` run succeeds under the Ready fixture, and the implementer handoff follows. Assert both Tech Lead work runs retain Ready-stage context and no Tech Lead git footer is posted.

- [ ] **Step 5: Run the integration binary**

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_workflow`

Expected: PASS, including unchanged direct implementer, In Review, In QA, and human approval cases.

- [ ] **Step 6: Commit**

Run: `git add server/tests/integration_workflow.rs && git commit -m "test(server): cover Ready Tech Lead workflow"`

### Task 6: M05 workflow documentation and smoke flow

**Files:**
- Modify: `docs/milestones/M05-workflow-and-collaboration.md`
- Modify: `e2e/smoke/m05-workflow.mjs`

- [ ] **Step 1: Document the ownership gate**

Describe `Ready` Tech Lead refinement, the no-code contract, required enabled implementer handoff, Ready auto-assign policy, run-start-owned `In Progress` transition, invalid-target system comments, and preserved clarification/resume. Update the pipeline to PM → Ready Tech Lead refinement → engineer implementation → review → QA → human approval. Keep informal Tech Lead requests under response-only `agentRequests`.

- [ ] **Step 2: Update the M05 smoke script**

Create a Tech Lead agent, expect the PM pending recommendation to name `tech_lead`, manually assign it, wait for one automatic backend implementer run, then continue the existing blocked clarification and final approval assertions.

- [ ] **Step 3: Check JavaScript syntax and docs diff**

Run: `node --check e2e/smoke/m05-workflow.mjs`

Expected: PASS.

- [ ] **Step 4: Commit**

Run: `git add docs/milestones/M05-workflow-and-collaboration.md e2e/smoke/m05-workflow.mjs && git commit -m "docs: describe Ready technical refinement gate"`

### Task 7: Verification and review

**Files:**
- Review all modified files.

- [ ] **Step 1: Format and run focused unit coverage**

Run: `cargo fmt --all -- --check`

Run: `cargo test -p coppice-server --lib`

Expected: PASS.

- [ ] **Step 2: Run the required workflow integration binary**

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_workflow`

Expected: PASS.

- [ ] **Step 3: Run Clippy**

Run: `cargo clippy --workspace -- -D warnings`

Expected: PASS.

- [ ] **Step 4: Review acceptance coverage and repository state**

Run: `git diff --check HEAD~6..HEAD` and `git status --short`.

Inspect the full diff for Ready-only scoping, no schema/API drift, exactly-one-run semantics, disabled target handling, and unchanged review/QA/final approval gates. Fix every critical or important finding and rerun affected checks.

- [ ] **Step 5: Commit any verification fixes**

Stage only ticket-owned changes, excluding `.agent/context.md`, and commit with a focused message. Do not push.
