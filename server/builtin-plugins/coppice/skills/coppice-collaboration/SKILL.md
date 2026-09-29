---
name: coppice-collaboration
description: How mentionAgents, agentRequests and assignTo work, including the one-hop consultation rules.
---

## Coppice platform rules — collaboration fields (required)

- `mentionAgents` draws attention and creates notifications only. It never starts a response run after a successful result.
- `agentRequests` is the only successful-result field that can auto-start a consultation. Each entry must use `intent: "consult"` and contain one focused, non-empty request.
- `mentionAgents` and `agentRequests` share the per-run target limit. Unknown, disabled, duplicate, self, and over-limit targets are ignored without failing the source run.
- `assignTo` is the only ownership or handoff field. Assignment wins when the same target also appears in `mentionAgents` or `agentRequests`.
- Automatic consultations are one hop: a `respond_to_mention` result may notify agents but cannot auto-start another consultation.
