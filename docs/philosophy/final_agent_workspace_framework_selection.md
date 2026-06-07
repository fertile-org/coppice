# Agent Workspace — Framework & Library Selection

> Companion document for the product design. This file only records which framework/library should be used for each technical area.

---

## 1. High-Level Stack

| Area | Selection | Notes |
|---|---|---|
| Backend server | **Rust** | Main orchestrator, API, workers, sandbox/session control. Low memory, safe process handling. |
| HTTP API | **Axum** | Lightweight, async, production-ready Rust web framework. |
| Async runtime | **Tokio** | Required for async HTTP, WebSocket, process streaming, workers. |
| Database | **PostgreSQL** | Primary relational database from v1. Avoid SQLite migration cost. |
| Vector search | **pgvector** | Store and query agent/project/team knowledge embeddings inside PostgreSQL. |
| Frontend | **React SPA** | Independent UI client consuming HTTP + WebSocket APIs. |
| Frontend build tool | **Vite** | Fast, simple SPA development/build. |
| Deployment | **Docker Compose** | Simple self-hosted deployment: server + Postgres + optional web container. |

---

## 2. Backend Libraries

| Responsibility | Library / Tool | Notes |
|---|---|---|
| HTTP routing | **axum** | API routes and WebSocket endpoints. |
| Middleware / layers | **tower**, **tower-http** | CORS, tracing, static file serving, request limits. |
| WebSocket | **axum WebSocket** | Live terminal stream, board events, run status updates. |
| Database access | **sqlx** | Async PostgreSQL queries, compile-time checked SQL if desired. |
| Migration | **sqlx migrate** | Simple migration workflow. |
| Serialization | **serde**, **serde_json** | API DTOs, config, result contracts. |
| Config | **figment** or **config-rs** | Load YAML/TOML/env configuration. Pick one; prefer `figment` for typed config. |
| Logging/tracing | **tracing**, **tracing-subscriber** | Structured logs for server, workers, agent runs. |
| Error handling | **thiserror**, **anyhow** | `thiserror` for domain errors, `anyhow` for worker/internal errors. |
| IDs | **uuid** | Entity IDs across tickets, agents, jobs, runs, knowledge items. |
| Time | **chrono** or **time** | Timestamps. Prefer `time` for newer Rust style, `chrono` if SQLx integration is easier. |
| Job queue | **Postgres-backed custom queue** | Use `agent_jobs` table. No Redis/BullMQ in v1. |
| Process execution | **tokio::process** | Run CLI tools, wrappers, scripts. |
| Git operations | **git CLI first** | Use shell commands first; avoid binding complexity. Consider `git2` later only if needed. |
| Terminal session | **tmux first** | Simple detach/reconnect/live viewing. PTY driver can be added later. |
| PTY future option | **portable-pty** | Optional later replacement/addition to tmux driver. |
| Secrets encryption | **age** or **ring** | Encrypt stored secrets. Simpler v1 can use OS/env-provided master key. |
| Password/auth hashing | **argon2** | Only if user login is implemented. |
| API auth | **session cookie or bearer token** | For self-hosted v1, simple bearer token is acceptable. |

---

## 3. Database & Knowledge

| Responsibility | Selection | Notes |
|---|---|---|
| Main DB | **PostgreSQL 16+** | Stable base version. |
| Vector extension | **pgvector** | Use official pgvector image or install extension manually. |
| Embedding storage | `vector(n)` column | Dimension depends on configured embedding model. |
| Metadata filtering | PostgreSQL columns/indexes | Filter by project, agent, scope, type, confidence, expiry before vector ranking. |
| Heavy files | Local filesystem | Store terminal logs, screenshots, diffs, artifacts outside DB. |
| Future object storage | S3-compatible storage | Optional later for attachments/artifacts. |

---

## 4. Agent Runtime & Sandbox

| Responsibility | Selection | Notes |
|---|---|---|
| CLI providers | Adapter pattern | Claude Code, Codex, OpenCode, OpenClaw, shell provider. |
| Provider execution | Command-based | Server prepares context files and invokes configured CLI. |
| Live session v1 | **tmux** | One tmux session per agent run. Stream via WebSocket. |
| Sandbox v1 | Process-level sandbox | Controlled env vars, working directory, command allowlist, timeout, logs. |
| Sandbox v2 | Rootless Docker/Podman | Stronger isolation when needed. |
| Sandbox v3 | gVisor / Firecracker / nsjail / bubblewrap | Future hard-isolation options. Not v1. |
| Command policy | Allowlist + denylist | Deny dangerous commands by default. |
| Secret injection | Environment variables | Inject only secrets allowed by agent capability/sandbox profile. |
| Resource limits | Timeout + output limit first | Add cgroup/container limits later. |

---

## 5. Frontend Libraries

| Responsibility | Library | Notes |
|---|---|---|
| UI framework | **React** | SPA only; server remains API-first. |
| Build tool | **Vite** | Fast dev/build. |
| Server state | **TanStack Query** | API cache, mutations, background refresh. |
| Routing | **TanStack Router** or **React Router** | Prefer TanStack Router if type-safety is desired. React Router is simpler/common. |
| Local UI state | **Zustand** | Only for UI state that is not server state. Use sparingly. |
| Kanban drag/drop | **dnd-kit** | Lightweight, flexible drag/drop. |
| Terminal UI | **xterm.js** | Live Agent Console. |
| Component primitives | **Radix UI** | Accessible primitives. |
| Styling | **Tailwind CSS** | Fast simple UI, avoid heavy component frameworks. |
| Component set | **shadcn/ui** | Optional thin layer on Radix + Tailwind. Use minimally. |
| Forms | **React Hook Form** | Agent config, sandbox profile, secrets, tickets. |
| Validation | **Zod** | Frontend schema validation; can also generate/share DTO shapes conceptually. |
| Markdown rendering | **react-markdown** | Ticket descriptions, comments, agent summaries. |
| Code highlighting | **Shiki** or **highlight.js** | Optional for diffs/log snippets. |

---

## 6. UI Shape

| Screen | Main Libraries Used |
|---|---|
| Board | React, TanStack Query, dnd-kit, shadcn/Radix components |
| Ticket Detail | React, TanStack Query, react-markdown, WebSocket events |
| Live Console | xterm.js, WebSocket stream |
| Agents | React Hook Form, Zod, TanStack Query |
| Knowledge | React, TanStack Query, simple table/list UI |
| Inbox / Signals | React, TanStack Query, simple list/detail UI |
| Sandbox / Capabilities / Secrets | React Hook Form, Zod, shadcn/Radix |

---

## 7. API & Realtime

| Responsibility | Selection | Notes |
|---|---|---|
| API style | REST JSON | Simple and easy for SPA/CLI clients. |
| Realtime board updates | WebSocket `/ws/events` | Ticket updates, comments, run status, notifications. |
| Live terminal stream | WebSocket `/ws/runs/:id/live` | Raw terminal frames or structured frames. |
| File upload | Multipart HTTP | Attach screenshots, logs, artifacts. |
| API docs | OpenAPI later | Optional. Can generate later from route definitions or maintain manually. |

---

## 8. Embedding Provider

| Responsibility | Selection | Notes |
|---|---|---|
| Provider abstraction | `EmbeddingProvider` interface | Do not hardcode one vendor. |
| Default v1 | OpenAI-compatible embeddings API | Works with many providers/proxies. |
| Config | Base URL + API key + model + dimension | Dimension must match pgvector column/index strategy. |
| Local future option | Ollama/local embedding model | Optional later. |

---

## 9. Suggested Repo Structure

```text
agent-workspace/
  server/
    src/
      api/
      domain/
      services/
      workers/
      providers/
      sandbox/
      sessions/
      knowledge/
      db/
      config/
    migrations/

  web/
    src/
      app/
      pages/
      features/
        board/
        tickets/
        agents/
        runs/
        knowledge/
        inbox/
        sandbox/
      components/
      api/
      ws/

  deploy/
    docker-compose.yml
    server.Dockerfile
    web.Dockerfile
```

---

## 10. Default v1 Decision Summary

Use this as the default implementation stack:

```text
Backend:
- Rust
- Axum
- Tokio
- SQLx
- PostgreSQL 16+
- pgvector
- tmux session driver
- Postgres-backed job queue
- local filesystem artifact storage

Frontend:
- React SPA
- Vite
- TanStack Query
- dnd-kit
- xterm.js
- Tailwind CSS
- Radix UI / shadcn/ui
- React Hook Form + Zod

Deployment:
- Docker Compose
- server + postgres
- optional separate web container, or serve SPA from Rust server
```

---

## 11. Things Not to Use in v1

Avoid these initially:

```text
- Kubernetes
- Redis queue
- Kafka/NATS
- microservices
- separate vector database
- Electron desktop app
- complex Jira-like workflow engine UI
- local LLM hosting requirement
- strong VM sandbox from day one
```

These can be added later only when the simple self-hosted version is stable.
