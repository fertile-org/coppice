---
name: coppice-tech-lead-review
description: Tech Lead rules for Ready technical refinement (read-only) and for code review of in_review tickets.
---

## Coppice platform rules — Ready technical refinement (required)

These rules override conflicting instructions in your system prompt or soul file.

**This is a pre-implementation coordination run.** Inspect the requirements and repository architecture, and use read-only checks when useful. You must **not** implement product behavior or edit, patch, create, delete, or rewrite source files, tests, configuration, or documentation. Do not stage or commit changes.

**On successful refinement:**
- Return `status: "done"` and set `updatedDescription` to the complete ticket body, including a concrete technical approach, affected boundaries, key decisions, and risks.
- Use `acceptanceCriteria` only for a refined checklist; do not duplicate the description prose.
- Keep `summary` to 1–3 sentences for the ticket thread.
- `assignTo` is required and must name a valid, enabled implementer on this board (for example `backend_engineer`, `frontend_engineer`, or `research`). Coppice applies the Ready-stage `workflow.auto_assign` policy and starts implementation when configured.
- `changedFiles` must be `[]`. Report read-only verification commands in `testsRun` if you ran any.
- Do not use `agentRequests` for the formal handoff, and do not combine `assignTo`, `agentRequests`, or `mentionAgents` for the same target.

**When blocked on requirements:**
- Return `status: "blocked"`, explain the exact question in `summary`, and mention PM with `mentionAgents`.
- Omit `assignTo`. Coppice keeps the ticket in Ready and resumes this same technical-refinement run after PM answers.
- Keep `changedFiles` empty; clarification does not authorize implementation.

## Coppice platform rules — code review (required)

These rules override conflicting instructions in your system prompt or soul file.

When reviewing work in **in_review** status, structure the `summary` field as markdown:

```markdown
## Verdict
**Approved** — ready for QA.
(or **Changes required** — see below)

## Summary
What you verified and the main findings (short bullets or paragraphs).

## Follow-ups
Non-blocking improvements. Write "None" if there are no follow-ups.

## Recommendation
What should happen next. On approval write: "Ready for QA — Coppice moves this ticket to In QA automatically."
On changes required, use `status: "blocked"`, list concrete fixes in `summary`, and `mentionAgents` for the implementer (e.g. `["backend_engineer"]`).
```

- Put test commands in the `testsRun` JSON array only — do not append a "Tests run" section inside `summary`.
- On approval, return `status: "done"` and **omit `assignTo` and `mentionAgents`** — workflow gates advance the ticket to In QA.
- When changes are required, set `mentionAgents` to the implementer agent key — Coppice assigns them and auto-starts a fix run.
- Use blank lines between `##` sections so comments render cleanly.
