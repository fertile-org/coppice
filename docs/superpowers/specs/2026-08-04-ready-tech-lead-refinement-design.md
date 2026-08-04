# Ready Tech Lead Refinement — Design

**Status:** Approved by ticket contract
**Date:** 2026-08-04
**Depends on:** Consultation-safe agent requests

## Problem

A Tech Lead assigned to a ticket in `Ready` currently receives the generic implementer contract. That contract authorizes source changes, requires git finalization, and reserves `assignTo` for PM refinement. The runtime therefore cannot distinguish formal pre-implementation technical refinement from implementation or an informal consultation.

## Considered approaches

1. **Specialize the existing `work_on_ticket` path by status and role (selected).** Add a Ready/Tech-Lead context contract and workflow branch while reusing assignment policy, auto-start, and clarification/resume machinery. This keeps the fixed board and preserves existing run history semantics.
2. **Add a `technical_refinement` job type.** This makes dispatch explicit but duplicates `work_on_ticket` lifecycle code and requires schema/API/provider changes that the ticket excludes.
3. **Add a refinement status or substatus.** This exposes the phase on the board but changes the fixed workflow model and editor, also outside scope.

## Context contract

Full-context `work_on_ticket` runs select technical refinement when the ticket is `Ready` and the assigned agent is a Tech Lead by preset key or role. The Ready-specific guidance is emitted before generic implementer guidance and:

- authorizes `updatedDescription`, `acceptanceCriteria`, and `assignTo`;
- requires a concrete technical approach and risks in the ticket update plus a concise thread summary;
- requires `changedFiles: []` and permits only inspection/read-only checks;
- prohibits implementation, source edits, staging, and commits;
- requires a successful result to hand ownership to an enabled implementer;
- preserves blocked PM clarification through `mentionAgents`.

PM guidance defines one intent per target: `assignTo` transfers or recommends ownership, `agentRequests` requests bounded consultation, and successful `mentionAgents` only notifies. A formal PM-to-Tech-Lead handoff uses only `assignTo` for that target.

## Workflow resolution

The workflow resolver recognizes Ready-stage Tech Lead refinement before generic success gates. A successful result remains in `Ready` and is handled as follows:

- A valid enabled implementer target follows `workflow.auto_assign`: immediate assignment when enabled, otherwise a pending recommendation.
- Missing, empty, unknown, disabled, or non-implementer targets leave status and assignment unchanged and add an actionable system comment listing available implementer keys.
- The refinement result never moves the ticket directly to `In Progress` or `In Review`.

With `auto_start_runs`, the existing orchestrator starts one `work_on_ticket` run for an immediately assigned implementer. The existing run-start gate owns `Ready → In Progress`. Successful refinement consultations are not auto-dispatched, so a valid handoff produces exactly one implementer run and an invalid handoff produces no run.

Blocked Ready-stage Tech Lead results bypass verification handoff logic. A PM mention creates the existing response job with the Tech Lead as `resume_agent_id`; the PM answer restores Tech Lead assignment and queues the same `work_on_ticket` job while status remains `Ready`. Context reconstruction therefore selects the same technical-refinement contract without a new job type.

## Git boundary

The worker identifies Ready-stage Tech Lead `work_on_ticket` runs and skips worktree git finalization. Consultation and In-QA QC guards remain unchanged. Context guidance and worker behavior independently enforce the no-implementation boundary.

## Mock fixtures and documentation

MockProvider resolves a Ready-specific fixture before its ordinary per-job fixture. The default PM fixture recommends Tech Lead ownership without a duplicate mention or consultation; the Ready Tech Lead fixture refines the plan and assigns the backend implementer. The ordinary Tech Lead `work_on_ticket` fixture remains a review result for `In Review`.

M05 documentation describes `Ready` as the technical-refinement ownership gate and distinguishes it from response-only consultation.

## Testing

- Context unit tests assert Ready technical-refinement guidance, allowed fields, empty changed files, implementation prohibition, absence of git instructions, PM intent separation, and unchanged review/QA contracts.
- Workflow unit tests cover Ready Tech Lead start behavior, auto/manual assignment policy, valid implementer selection, missing/unknown/disabled-or-unavailable/non-implementer notices, and existing gates.
- Worker unit tests cover Ready Tech Lead git-finalization suppression and unchanged implementer/reviewer/QC behavior.
- `integration_workflow` covers the PM → Tech Lead → implementer pipeline, exact single-run auto-start, invalid handoffs, and blocked clarification/resume under Ready rules.

## Scope

No database migration, status, substatus, column, job type, API shape, or workflow editor change is required.
