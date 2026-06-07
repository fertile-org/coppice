# Coppice Implementation Milestones

**Coppice** — grow an agent team from shared roots.

Coppice is a lightweight, self-hosted workspace where AI agents work through tickets, communicate through comments, learn from project history, operate inside sandboxes, and proactively raise engineering signals.

## Monorepo

Coppice is a **monorepo**. Each part has its own top-level folder:

```text
server/   # Rust API + workers
web/      # React SPA
cli/      # Rust operator CLI (`coppice`)
deploy/   # docker-compose + Dockerfiles
e2e/      # browser test scripts
docs/     # philosophy + milestone specs
```

See the [milestone strategy](../superpowers/specs/2026-06-07-coppice-milestone-strategy-design.md) for full layout and package boundaries.

## How to use these docs

1. Read the [milestone strategy](../superpowers/specs/2026-06-07-coppice-milestone-strategy-design.md) for overall decisions.
2. Implement milestones **in order** — each builds on the prior.
3. Do not skip acceptance criteria; cumulative `docker compose up` must work after each milestone.
4. Use `MockProvider` for all automated agent tests until real CLI adapters are configured manually.

## Milestones

| # | Doc | Goal |
|---|-----|------|
| M01 | [M01-foundation.md](./M01-foundation.md) | Infra, auth API, mock provider, CI |
| M02 | [M02-workspace-and-board.md](./M02-workspace-and-board.md) | Login UI, board, tickets, agents CRUD |
| M03 | [M03-agent-execution.md](./M03-agent-execution.md) | Job queue, worktrees, mock agent runs |
| M04 | [M04-live-console.md](./M04-live-console.md) | Live terminal, WebSocket, log artifacts |
| M05 | [M05-workflow-and-collaboration.md](./M05-workflow-and-collaboration.md) | Workflow rules, mentions, final review |
| M06 | [M06-knowledge-and-learning.md](./M06-knowledge-and-learning.md) | pgvector, retrieval, learning inbox |
| M07 | [M07-trust-and-signals.md](./M07-trust-and-signals.md) | Sandbox, secrets, signals, git/PR |

## Philosophy references

- [Product design](../philosophy/final_agent_workspace_product_design.md)
- [Framework selection](../philosophy/final_agent_workspace_framework_selection.md)
