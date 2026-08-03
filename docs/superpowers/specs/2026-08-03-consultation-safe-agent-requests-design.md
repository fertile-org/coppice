# Consultation-Safe Agent Requests — Design

**Status:** Approved by ticket contract  
**Date:** 2026-08-03  
**Depends on:** M05 workflow and collaboration

## Problem

Successful full-context runs currently turn every persisted `mentionAgents` target into a `respond_to_mention` run. That conflates attention with execution, misses pending assignment recommendations during deduplication, and gives responders a generic implementation-capable context.

## Contract

Collaboration has three distinct signals:

- `mentionAgents` records attention and creates notifications. It never starts a run after a successful agent result.
- `agentRequests` records a bounded, non-empty `consult` request and may start one `respond_to_mention` run from a successful `work_on_ticket` result.
- `assignTo` records ownership or handoff. Applied assignments and pending assignment recommendations take precedence over consultation requests to the same agent.

Blocked `work_on_ticket` mentions retain the M05 clarification and resume behavior.

## Result model and durable request format

`AgentRunResult::Done` gains `agentRequests`, with entries containing `agentKey`, `intent`, and `request`. The server accepts only `intent: "consult"`, trims empty keys and requests, rejects requests above the server-owned character bound, resolves enabled targets, rejects self-targets, deduplicates aliases by agent ID, and applies the shared per-run target limit across `mentionAgents` and `agentRequests`.

The source comment renders each accepted consultation request for humans and carries a one-line Coppice metadata marker containing the same structured values. The marker allows a later worker to recover the exact request through the existing `trigger_comment_id`; no database column is added.

## Dispatch and lifecycle

Only successful full-context `work_on_ticket` runs may auto-dispatch accepted `agentRequests`. Successful `respond_to_mention` runs persist mentions and notifications but mark their own request mentions handled without dispatch, enforcing one-hop automatic collaboration. Attention-only mentions remain durable and pending but are excluded from deferred scheduling.

Before dispatch, the orchestrator builds an ownership set from workflow handoff jobs, immediate assignee changes, and `pending_assign_recommendation`. A consultation whose target is in that set is handled without creating a response run.

Blocked work mentions continue through the existing workflow-generated response jobs with `resume_agent_id`, waiting substatus, clarification-round increment, and original-agent resume.

## Consultation context and defensive application

A full-context `respond_to_mention` run receives a dedicated document layout:

1. exact triggering request;
2. Coppice-owned response-only rules;
3. ticket context and thread;
4. role-specific prompt;
5. response-only JSON contract.

The rules authorize answering, code inspection, and read-only checks only. They prohibit implementation, edits, commits, assignment, and workflow movement.

The server enforces the boundary independently of provider compliance. For every `respond_to_mention` result it ignores ticket description and acceptance-criteria updates, split proposals, assignment and pending-recommendation effects, status/substatus mutations, and git finalization. A blocked response records only its explanatory comment and blocked run status; ticket lifecycle state remains unchanged.

## Testing

- Unit tests cover request normalization, metadata round-trip, shared target limiting, attention-only dispatch, assignment precedence, one-hop suppression, dedicated context ordering/rules, and response-only result application.
- Orchestrator tests cover persistence with auto-start enabled/disabled, pending-recommendation precedence, defensive lifecycle invariance, clarification resume, and verification handoff deduplication.
- `integration_agent_mentions` covers the successful attention path, one-hop consultation path, PM-to-Tech-Lead pending-assignment regression, and durable request behavior.
- Existing workflow integration coverage continues to protect blocked clarification/resume and QC defect handoffs.

## Scope

No schema, board-column, or frontend workflow change is required. Filesystem-level sandbox enforcement remains M07 scope.
