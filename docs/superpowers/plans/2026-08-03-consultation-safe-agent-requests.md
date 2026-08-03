# Consultation-Safe Agent Requests Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Separate attention mentions, consultation requests, and ownership so automatic agent collaboration is bounded, consultation-only, and one hop.

**Architecture:** Add a typed `agentRequests` result field plus a focused service for validation and durable comment metadata. The orchestrator persists a shared bounded target set but dispatches only validated consultation requests, with ownership precedence; the worker/context builder supply a response-only prompt and defensively suppress all ticket and git mutations for response runs.

**Tech Stack:** Rust, Axum services, SQLx/PostgreSQL integration tests, Serde JSON, Markdown context generation, MockProvider fixtures.

---

### Task 1: Result model and durable consultation requests

**Files:**
- Modify: `server/src/providers/mod.rs`
- Create: `server/src/services/agent_request.rs`
- Modify: `server/src/services/mod.rs`
- Modify: `server/src/services/result_contract.rs`

- [ ] **Step 1: Write failing unit tests**

Add tests that deserialize `agentRequests`, reject empty/non-consult/over-bound entries without failing the result, preserve an exact multiline request through comment metadata, and include request targets in comment mentions.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p coppice-server agent_request --lib`
Expected: FAIL because `AgentRequest` and durable metadata helpers do not exist.

- [ ] **Step 3: Implement the minimal result and helper types**

Define `AgentRequest { agent_key, intent, request }`, a server-owned maximum character count, normalization helpers, deterministic human-readable comment formatting, and a one-line JSON metadata parser. Add `agent_requests` to the `Done` variant with Serde camelCase mapping.

- [ ] **Step 4: Make result application include durable requests**

Append accepted requests and their metadata to done comments, and combine request keys with `mentionAgents` in the comment mention list. Add a response-only application function that retains run status, summary, and collaboration fields while returning an empty ticket update.

- [ ] **Step 5: Run focused tests**

Run: `cargo test -p coppice-server agent_request --lib`
Expected: PASS.

### Task 2: Consultation-only context

**Files:**
- Modify: `server/src/services/context_builder.rs`
- Modify: `server/src/workers/job_worker.rs`

- [ ] **Step 1: Write a failing context test**

Build a response context containing a multiline request and assert the exact request precedes ticket context, Coppice response-only rules precede the role prompt, and the contract omits mutation fields.

- [ ] **Step 2: Run the context test to verify failure**

Run: `cargo test -p coppice-server context_builder::tests::consultation --lib`
Expected: FAIL because full contexts have no consultation mode.

- [ ] **Step 3: Implement the dedicated response context**

Extend `ContextInput` with an optional consultation request. When present, render request-first response rules, then ticket/thread context, role prompt, sandbox note, and response-only done/blocked JSON examples. Do not inject role workflow or git-commit guidance.

- [ ] **Step 4: Wire trigger recovery in the worker**

For `respond_to_mention`, load the trigger comment, parse a target-matching structured request by resolved agent ID, and fall back to the exact legacy blocked-clarification comment body. Apply the response-only result path and keep git finalization restricted to `work_on_ticket`.

- [ ] **Step 5: Run focused tests**

Run: `cargo test -p coppice-server context_builder::tests::consultation --lib`
Expected: PASS.

### Task 3: Safe persistence and one-hop dispatch

**Files:**
- Modify: `server/src/services/run_orchestrator.rs`
- Modify: `server/src/services/mention_service.rs`
- Modify: `server/src/services/workflow_service.rs`

- [ ] **Step 1: Replace old dispatcher expectations with failing tests**

Cover attention-only mentions producing no job, work consultation producing one response job, response consultation producing no chained job, pending recommendation suppressing response dispatch, and shared valid-target limiting/deduplication.

- [ ] **Step 2: Run focused orchestration tests to verify failure**

Run: `cargo test -p coppice-server run_orchestrator::tests::successful --lib`
Expected: FAIL under the mention-driven dispatcher.

- [ ] **Step 3: Implement target selection and ownership precedence**

Resolve enabled agent keys to IDs, exclude source/self and duplicates, apply `MAX_MENTIONS_PER_RUN` across attention and request targets, persist one mention per selected target, and pass the accepted consultation ID set into dispatch. Add pending recommendation targets to the ownership set.

- [ ] **Step 4: Enforce one hop and response lifecycle invariance**

Dispatch only successful full-context `work_on_ticket` consultations. Mark request mentions from successful response runs handled, never enqueue them, and restrict deferred scheduling to comments containing a target-matching durable request. Force response workflow action and ticket-update application to no-op for both succeeded and blocked results.

- [ ] **Step 5: Preserve blocked work and verification handoffs**

Leave blocked `mentionAgents` workflow jobs and `resume_agent_id` semantics intact. Ensure verification handoff work jobs win over any collaboration mention and do not create a response run.

- [ ] **Step 6: Run orchestration unit tests**

Run: `cargo test -p coppice-server run_orchestrator --lib`
Expected: PASS.

### Task 4: Fixtures, integration regressions, and M05 documentation

**Files:**
- Modify: `fixtures/agent-responses/research/work_on_ticket.json`
- Modify: `fixtures/agent-responses/frontend_engineer/work_on_ticket.json`
- Modify: `fixtures/agent-responses/dba/respond_to_mention.json`
- Modify: `server/tests/integration_agent_mentions.rs`
- Modify: `server/tests/integration_workflow.rs` only if fixture assertions require it
- Modify: `docs/milestones/M05-workflow-and-collaboration.md`

- [ ] **Step 1: Update MockProvider fixtures**

Use `agentRequests` for the successful work consultation fixture, retain `mentionAgents` for attention and verification handoffs, and make response fixtures demonstrate that follow-up attention does not chain.

- [ ] **Step 2: Write integration regressions**

Cover attention mention persistence with zero response runs, consultation persistence and exactly one linked response run, disabled auto-start, PM `assignTo` plus same-target request under Backlog auto-assign gate, and response one-hop behavior.

- [ ] **Step 3: Update M05 collaboration semantics**

Document attention versus consultation versus ownership, bounded request shape, request-first response context, defensive response-only application, one-hop automatic collaboration, and preserved blocked clarification behavior.

- [ ] **Step 4: Run the required integration binary**

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_agent_mentions`
Expected: PASS.

### Task 5: Verification and review

**Files:**
- Review all modified files.

- [ ] **Step 1: Format and run unit coverage**

Run: `cargo fmt --all -- --check` and `cargo test -p coppice-server --lib`
Expected: PASS.

- [ ] **Step 2: Run focused workflow regression**

Run: `cargo test -p coppice-server --features embedded-test-db --test integration_workflow`
Expected: PASS, including blocked clarification/resume and QC defect handoff coverage.

- [ ] **Step 3: Run Clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: PASS.

- [ ] **Step 4: Review the diff against every acceptance criterion**

Inspect `git diff --check`, changed-file status, result contract compatibility, and test evidence. Fix all critical or important findings.

- [ ] **Step 5: Commit the implementation**

Stage only ticket-owned changes, excluding `.agent/context.md`, and commit with `fix(server): make agent consultations response-only`.
