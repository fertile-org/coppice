# M05 — Workflow & Collaboration

## Goal

Inter-agent coordination through ticket comments, `@mentions`, explicit workflow rules, clarification/resume flows, and the human final review gate — the full PM → Tech Lead → Engineer → Review → QA → Human pipeline.

## Product scope

- YAML workflow rules loaded from config (product design §8.2)
- Events: `on_ticket_created`, `on_agent_done`, status transitions
- `@agent-name` mention parsing in comments → `ticket_mentions` records → jobs for mentioned agents
- Job types: `work_on_ticket`, `respond_to_mention`, `review_ticket`, `qa_ticket`
- Clarification flow: agent returns blocked + mention → substatus waiting_for_* → resume job after answer
- Communication limits: max clarification rounds, max mentions per run, escalate to human (product design §9.4)
- Rich substatuses displayed on cards (product design §5.1)
- Columns used: full set including Wait for Final Review
- Human **Final Approve** action moves ticket to Done
- Resolve Blocker action for non-capability blockers
- Mock agents scripted with different roles return role-appropriate result contracts in tests

## Out of scope

- Missing-capability blocker UI (M07) — capability blockers may set status but guided unblock is M07
- Knowledge-informed context (M06)
- Proactive signals (M07)

## Dependencies

- M02: comments, board, agents
- M03: job queue, runs, result contract
- M04: WS events (board updates on workflow transitions)

## Architecture notes

### New server modules

```text
server/src/
  services/
    workflow_engine.rs
    mention_service.rs
  domain/
    mention.rs
    workflow_rule.rs
  workers/
    job_worker.rs          # extended job types
  config/
    workflow.yaml
```

### New database tables

```text
ticket_mentions
workflow_rules          # or file-only with DB audit log
blockers                # non-capability blockers
```

### Workflow config example

See product design §8.2 — loaded at server start, validated on boot.

### Mention flow

```text
Comment with @pm-agent
  → Mention record (pending)
  → agent_jobs: respond_to_mention
  → ticket substatus: waiting_for_pm_agent
  → PM mock run → clarification_answer comment
  → resume job for original assignee
```

### API additions

```text
POST /api/tickets/:id/final-approve
POST /api/tickets/:id/resolve-blocker
POST /api/mentions/:id/ignore
```

## Docker Compose delta

No new services. Optional workflow config mount:

```yaml
  server:
    volumes:
      - ./deploy/workflow.yaml:/etc/coppice/workflow.yaml:ro
```

## Testing strategy

### Unit tests

- Workflow rule matcher: on_ticket_created assigns PM
- Mention regex/parser extracts agent IDs
- Clarification round counter; escalation at limit
- on_agent_done transitions per agent role

### Integration tests

- Full pipeline with chained MockProvider fixtures:
  PM → Ready → Tech Lead → In Progress → Engineer → In Review → TL review → In QA → QC → Wait for Final Review
- Mention: engineer blocks with @pm-agent → PM job → answer → engineer resume job
- Communication limit exceeded → escalates to waiting_for_human
- Final Approve requires ticket in Wait for Final Review

### E2E smoke (CI)

`e2e/smoke/m05-workflow.spec`:

1. Create ticket with label triggering PM assignment (or manual assign PM mock)
2. Run through scripted mock sequence to Wait for Final Review
3. Click Final Approve → ticket in Done column

### E2E full (local)

- @mention visible in comment thread; mentioned agent receives job
- Blocked card badge shows substatus text
- Ignore mention action

## Acceptance criteria

- [ ] Workflow rules auto-assign and transition tickets without manual drag for agent-done paths
- [ ] Mentions create jobs and update substatus
- [ ] Clarification/resume cycle works with round limits
- [ ] Human Final Approve is required before Done
- [ ] CI smoke E2E passes full mock pipeline

## References

- Product design §8 (workflow), §9 (comments, mentions, clarification), §18.2 (approval gates)
- Product design §5.1 (substatus display)
