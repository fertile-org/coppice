# M02 — Workspace & Board Design Spec

**Date:** 2026-06-07  
**Status:** Draft — pending user review  
**Product:** Coppice — grow an agent team from shared roots.

**Depends on:** M01 Foundation (session auth API, Postgres, MockProvider, CI, Compose)  
**Milestone doc:** `docs/milestones/M02-workspace-and-board.md`

## Purpose

M02 delivers the first full-stack Coppice experience: login-gated React SPA, multi-project workspace, Trello-like board with fast fullscreen ticket drawer, comments with attachments, agent configuration from presets, and admin user management. Agents do not run yet (M03).

This spec captures design decisions from brainstorming (2026-06-07) and extends the milestone doc with concrete data models, API contracts, UX patterns, release packaging, and testing requirements.

## Brainstorming decisions

| Topic | Decision |
|-------|----------|
| Projects | Minimal multi-project: picker in shell, create/switch, each project has board + repos |
| Ticket detail | Fullscreen overlay drawer; board route stays mounted; close = strip URL query (instant back) |
| Substatus | Generic enum + JSONB metadata on cards; never agent-specific status strings |
| Board sync | Hybrid: optimistic drag/status; refetch ticket list when drawer closes |
| Compose dev | Split: Vite `:5173` + API `:8080` with HMR |
| Production release | `vite build` → static assets shipped in GitHub release tarball alongside `coppice-server` binary |
| Users | Admin creates users (email + password); members have same workspace access; only admin manages users |
| Implementation order | Domain-ordered vertical slices (API + tests per slice, then UI modules) |

## Out of scope (unchanged from milestone)

- Agent runs and job queue (M03)
- Workflow rules, mention-driven jobs (M05)
- WebSocket realtime (M04)
- Knowledge, capabilities, secrets, signals (M06–M07)
- Live Console tab
- Ticket search/filter (optional in milestone; deferred)

---

## Architecture overview

### Monorepo delta

```text
web/                          # NEW — Vite React SPA (package.json, src/, vitest)
server/migrations/002_workspace.sql
server/src/api/               # projects, repos, tickets, comments, agents, attachments, users
server/src/domain/            # + project, repo, ticket, comment, agent, attachment, substatus
server/src/services/          # + ticket, comment, agent, project, user services
server/src/storage/           # attachment_store.rs — filesystem under /data/artifacts
deploy/Dockerfile.web
deploy/docker-compose.yml     # + web service, artifact_data volume on server
e2e/smoke/m02-board.spec      # CI smoke
.github/workflows/ci.yml      # + web unit tests, smoke E2E job (if feasible in CI)
```

### Server module layout

Extends M01 patterns (`api` → `services` → SQLx, session + CSRF middleware):

```text
server/src/
  api/
    projects.rs, repos.rs, tickets.rs, comments.rs
    agents.rs, attachments.rs, users.rs
  domain/
    project.rs, repo.rs, ticket.rs, comment.rs, agent.rs, attachment.rs, substatus.rs
  services/
    project_service.rs, ticket_service.rs, comment_service.rs
    agent_service.rs, user_service.rs
  storage/
    attachment_store.rs
  middleware/
    admin.rs                    # AdminUser extractor (role == admin)
```

### Implementation approach

**Domain-ordered vertical slices (recommended):**

1. Migration `002_workspace.sql` + agent preset seed
2. Projects & repos API + integration tests
3. Tickets API (status, substatus, assign) + integration tests
4. Comments & attachments API + integration tests
5. Agents API + preset seed tests
6. Users API (admin-only) + integration tests
7. SPA shell: design tokens → auth → project picker
8. Board + optimistic DnD + fullscreen drawer
9. Agents UI + Users settings (admin)
10. Compose web service + release packaging + E2E smoke

Each slice leaves the repo runnable and tested before the next.

---

## Data model

Migration `002_workspace.sql` adds the following tables.

### `projects`

| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | |
| name | TEXT NOT NULL | |
| slug | TEXT NOT NULL UNIQUE | URL-safe |
| created_at | TIMESTAMPTZ | |

### `repos`

| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | |
| project_id | UUID FK → projects | |
| name | TEXT NOT NULL | |
| remote_url | TEXT NULL | |
| default_branch | TEXT NOT NULL DEFAULT 'main' | |
| created_at | TIMESTAMPTZ | |

### `agents`

M02 subset — no capabilities, sandbox, or secrets yet.

| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | |
| name | TEXT NOT NULL | |
| role | TEXT NOT NULL | e.g. "PM", "DBA" |
| skills | TEXT[] NOT NULL DEFAULT '{}' | |
| responsibilities | TEXT[] NOT NULL DEFAULT '{}' | |
| system_prompt | TEXT NOT NULL | |
| provider_id | TEXT NOT NULL DEFAULT 'mock' | |
| enabled | BOOLEAN NOT NULL DEFAULT true | |
| preset_source | TEXT NULL | preset key if created from template |
| created_at, updated_at | TIMESTAMPTZ | |

### `agent_presets` (seed data)

Read-only templates. Seeded in migration with 10 presets from product design §6.2:

PM, Technical Lead, Frontend Engineer, Backend Engineer, DBA, QC, Reviewer, DevOps, Security, Research.

Each preset row: `id`, `key`, `role`, `skills[]`, `responsibilities[]`, `system_prompt_template`.

### `tickets`

| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | |
| project_id | UUID FK | |
| repo_id | UUID FK NULL | |
| title | TEXT NOT NULL | |
| description | TEXT NOT NULL DEFAULT '' | markdown |
| status | TEXT NOT NULL | board column enum (see below) |
| substatus | TEXT NULL | generic enum (see below) |
| substatus_metadata | JSONB NULL | structured context |
| priority | TEXT NULL | low \| medium \| high \| critical |
| assignee_agent_id | UUID FK NULL → agents | |
| owner_user_id | UUID FK NULL → users | |
| branch_name | TEXT NULL | badge only in M02 |
| created_by | TEXT NOT NULL | human \| agent \| system |
| created_by_id | UUID NULL | |
| created_at, updated_at | TIMESTAMPTZ | |

Deferred nullable columns (add only if trivial): `reviewer_agent_id`, `worktree_path`, `parent_ticket_id`, `source_signal_id`.

**Board column enum (`status`):**

```text
backlog | ready | in_progress | in_review | in_qa |
wait_for_final_review | done | blocked
```

**Substatus enum (`substatus`) — generic only:**

| Value | Card label | Required metadata |
|-------|------------|-------------------|
| `waiting_for_agent` | Waiting for agent | `{ "agentId": "uuid" }` |
| `waiting_for_human` | Waiting for you | `{ "reason"?: string }` optional |
| `waiting_for_owner` | Waiting for owner | `{ "reason"?: string }` optional |
| `waiting_for_ci` | Waiting for CI | `{ "checkName"?: string }` optional |
| `blocked_by_missing_capability` | Blocked — capability | `{ "capability": string }` |
| `blocked_by_missing_secret` | Blocked — secret | `{ "secretKey": string }` |
| `blocked_by_permission` | Blocked — permission | `{ "detail"?: string }` optional |
| `blocked_by_error` | Blocked — error | `{ "summary"?: string }` optional |
| `null` | (column badge only) | — |

**Rules:**

- Never store display strings like `"Waiting for PM Agent"` in `substatus`.
- Server validates metadata schema per substatus value; reject mismatches with `400`.
- Server computes `substatusDisplay: { label, detail }` on read (e.g. resolve `agentId` → agent name for `detail`).
- Hard reject `status = done` when `substatus` is any `waiting_*` or `blocked_*` value.

### `ticket_comments`

| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | |
| ticket_id | UUID FK | |
| author_type | TEXT NOT NULL | human \| agent \| system |
| author_id | UUID NULL | user or agent id |
| body | TEXT NOT NULL | markdown |
| intent | TEXT NOT NULL | see product design §9.1 enum |
| mentions | JSONB NOT NULL DEFAULT '[]' | stored only; not processed in M02 |
| attachment_ids | UUID[] NOT NULL DEFAULT '{}' | |
| created_at | TIMESTAMPTZ | |

### `attachments`

| Column | Type | Notes |
|--------|------|-------|
| id | UUID PK | |
| filename | TEXT NOT NULL | original name |
| content_type | TEXT NOT NULL | |
| size_bytes | BIGINT NOT NULL | |
| storage_path | TEXT NOT NULL | under `/data/artifacts/` |
| uploaded_by | UUID FK → users | |
| created_at | TIMESTAMPTZ | |

Files stored at `/data/artifacts/{attachment_id}/{filename}`. Max upload size: configurable, default **10 MB** (`COPPICE_MAX_UPLOAD_BYTES`).

### Users (extends M01)

Existing `users.role`: bootstrap user is `admin`. New users created via API get `role = member`. M02 permissions:

- **admin:** `GET/POST /api/users`, all other routes
- **member:** all routes except user management

No fine-grained RBAC in M02.

---

## API design

### Auth & middleware

Same as M01:

- Session cookie `coppice_session` (httpOnly, secure in prod)
- CSRF header `X-CSRF-Token` on all mutating requests to protected routes
- `AuthUser` extractor on protected routes
- New `AdminUser` extractor: wraps `AuthUser`, returns `403` if `role != admin`

### Public routes

```text
GET  /health
POST /api/auth/bootstrap
POST /api/auth/login
```

### Protected routes (session required)

```text
GET  /api/auth/me
POST /api/auth/logout

GET  /api/projects
POST /api/projects                              { name, slug? }
GET  /api/projects/:projectId
PATCH /api/projects/:projectId                  { name?, slug? }

GET  /api/projects/:projectId/repos
POST /api/projects/:projectId/repos            { name, remoteUrl?, defaultBranch? }
GET  /api/repos/:repoId
PATCH /api/repos/:repoId
DELETE /api/repos/:repoId

GET  /api/projects/:projectId/tickets          ?status=&assigneeAgentId=
POST /api/projects/:projectId/tickets         { title, description?, repoId?, priority?, ... }
GET  /api/tickets/:ticketId
PATCH /api/tickets/:ticketId                    partial update
PATCH /api/tickets/:ticketId/status             { status, substatus?, substatusMetadata? }
POST /api/tickets/:ticketId/assign              { agentId: uuid | null }

GET  /api/tickets/:ticketId/comments
POST /api/tickets/:ticketId/comments            { body, intent?, attachmentIds? }

GET  /api/agents
POST /api/agents                                { name, presetId?, ...fields }
GET  /api/agents/:agentId
PATCH /api/agents/:agentId
DELETE /api/agents/:agentId
GET  /api/agent-presets

POST /api/attachments                           multipart/form-data
GET  /api/attachments/:attachmentId             stream file
```

### Admin-only routes

```text
GET  /api/users
POST /api/users                                 { email, password }
```

### Response conventions

- JSON camelCase
- Lists: `{ "items": [ ... ] }`
- Errors: `{ "error": "validation_error", "message": "..." }` with appropriate HTTP status
- Ticket responses include computed `substatusDisplay: { label, detail? }`
- Ticket list includes `lastActivityAt` (max of ticket.updated_at, latest comment.created_at)

### Status transitions

No workflow engine in M02. `PATCH .../status` accepts any valid column directly (drag or form). Server validates enums and metadata only.

---

## Frontend design

### Mandatory: frontend-design skill

**Before implementing any UI in `web/`, invoke the `frontend-design` skill.** Do not scaffold pages with default shadcn/Tailwind boilerplate and skip visual design.

The skill must produce:

1. **Design direction** — aesthetic aligned with Coppice (“grow an agent team from shared roots”): intentional tone, not generic AI dashboard styling
2. **Theme tokens** — CSS variables in `web/src/styles/tokens.css` (or equivalent):
   - Color palette (background, surface, border, text, accent, status column colors, substatus badge variants)
   - Typography scale (display + body font choices — avoid Inter/Roboto defaults)
   - Spacing, radius, shadow, motion duration tokens
3. **Component styling baseline** — Tailwind theme extension mapping tokens; minimal shadcn overrides consistent with direction
4. **`web/DESIGN.md`** — short living doc: direction, fonts, token reference, do/don’t for M02 screens

Implementation plan task 7 (SPA shell) must explicitly start with frontend-design output before login/project picker code.

### Stack

| Layer | Choice |
|-------|--------|
| Build | Vite + React + TypeScript |
| Server state | TanStack Query |
| Routing | React Router v6 |
| DnD | dnd-kit |
| Forms | React Hook Form + Zod |
| Styling | Tailwind CSS + shadcn/ui (minimal, themed via tokens) |
| Markdown | react-markdown |

### Routes

```text
/login
/projects                              project picker + create
/projects/:projectId/board             kanban (primary workspace)
/settings/users                        admin only
/agents                                global agent list + create/edit
```

- After login → `/projects` or last project from `localStorage`
- Admin-only nav link to `/settings/users`

### Fullscreen drawer (performance-critical)

- Board route **never unmounts** when viewing a ticket
- URL pattern: `/projects/:projectId/board?ticket=:ticketId`
- Open drawer when `ticket` query present; **close = navigate without query** (no route transition, no board remount)
- Drawer: `fixed inset-0` overlay above board; board remains visible underneath (dimmed) or fully hidden — design choice in frontend-design pass, but mount behavior is fixed
- Avoid Jira-like full page loads; no loading gate on the board when closing drawer

### TanStack Query cache (hybrid sync)

| Action | Behavior |
|--------|----------|
| Load board | `queryKey: ['tickets', projectId]`, staleTime ~30s |
| Drag column | Optimistic cache update → `PATCH .../status` → rollback on error |
| Open drawer | Seed from board cache; fetch `['ticket', id]` + `['comments', id]` if stale |
| Edit in drawer | Update detail cache; do not invalidate board list until close |
| Close drawer | Invalidate `['tickets', projectId]`; refetch to merge comments / last activity |

### Board UI

- Horizontal scroll; 8 fixed columns matching status enum
- Column headers show count; Backlog has quick-add
- Card: title, assignee chip, status + substatus badges (using `substatusDisplay`), repo/branch badge, relative last activity
- dnd-kit drag between columns → optimistic status PATCH

### Drawer tabs

1. **Description** — markdown, priority, repo, branch, assign agent
2. **Comments** — thread (human / agent / system badges), markdown compose, file attach
3. **Metadata** — substatus picker (generic enum) + conditional metadata fields (agent picker, secret key, capability text, etc.)

### Auth client

- `fetch(..., { credentials: 'include' })`
- Store CSRF from login response; attach `X-CSRF-Token` on mutations
- Boot: `GET /api/auth/me` → redirect `/login` on 401

### Dev API proxy

Vite dev server proxies `/api` → backend so session cookies work without cross-origin issues. Env: `VITE_API_URL` or compose `web.environment`.

---

## Docker Compose & release

### Development (`docker compose up`)

```yaml
services:
  postgres:   # unchanged from M01
  server:
    ports: ["8080:8080"]
    volumes:
      - artifact_data:/data/artifacts
    environment:
      DATABASE_URL: ...
  web:
    build: deploy/Dockerfile.web
    ports: ["5173:5173"]
    depends_on: [server]
    environment:
      VITE_API_URL: http://server:8080   # used by proxy target

volumes:
  artifact_data:
```

`Dockerfile.web`: Node image, `npm ci`, `npm run dev -- --host 0.0.0.0`.

### Production / GitHub release

Not a second compose profile for M02. Release artifact is a **tarball**:

```text
coppice-{version}-linux-amd64.tar.gz
  coppice-server          # Rust binary
  coppice-cli             # optional, same release
  web/dist/               # output of `npm run build` in web/
  deploy/config/default.yaml
  README-RELEASE.md
```

Build pipeline (CI or release workflow):

1. `cargo build --release -p coppice-server -p coppice-cli`
2. `cd web && npm ci && npm run build`
3. Package tarball

Server config: `server.static_dir` (or env `COPPICE_STATIC_DIR`) pointing to `web/dist/`. Server serves SPA + fallback `index.html` for client routes. Single port `:8080` in production.

Makefile targets: `make web-build`, `make release-tar` (stub in M02 if full release workflow deferred to later PR).

---

## Testing strategy

### Server unit tests

- Substatus enum + metadata validation (required fields, reject invalid combos)
- `substatusDisplay` resolution (agent name lookup)
- Reject `done` with active blocked/waiting substatus
- Agent preset seed count and keys
- Attachment path generation and size limit

### Server integration tests (`server/tests/`)

Use compose Postgres; serialize DB-modifying tests (same `TEST_DB_LOCK` pattern as M01 auth tests).

- Project → repo → ticket → comment happy path
- Status PATCH + invalid column rejected
- Assign agent; verify `assignee_agent_id`
- Attachment upload → file on temp dir + metadata row
- Agent CRUD from preset template
- Admin creates user; member gets 403 on `POST /api/users`
- All entity routes return 401 without session

### Web unit tests (Vitest)

- Zod schemas: ticket create, agent form, substatus metadata per type
- Board column ordering helper
- Substatus badge label helper (uses server display shape)

### E2E smoke (CI, `e2e/smoke/m02-board.spec`)

Backend via `docker compose up`. Script:

1. Login → land on project picker → open/create project → board
2. Create ticket in Backlog
3. Drag ticket to Ready
4. Open ticket drawer → add human comment → verify visible
5. Close drawer → board still mounted (no full reload spinner)

### E2E full (local, `make e2e`)

- Create agent from PM preset; assign to ticket
- Upload attachment on comment
- Edit substatus + metadata in Metadata tab; verify card badge
- Admin creates second user (optional flow)
- All 8 columns render

---

## Acceptance criteria

- [ ] `frontend-design` skill run; `web/DESIGN.md` + theme tokens exist before feature UI
- [ ] Login gates SPA; session persists across refresh
- [ ] Multi-project picker; per-project board and repos
- [ ] Full board CRUD via UI and API; optimistic column drag
- [ ] Fullscreen drawer opens/closes without unmounting board
- [ ] Generic substatus + metadata on cards and Metadata tab
- [ ] Comments and attachments on ticket detail
- [ ] Agents CRUD from presets; manual assign
- [ ] Admin can create users; members cannot
- [ ] `docker compose up` → Vite + API + Postgres working
- [ ] Release build produces server binary + `web/dist/` documented
- [ ] CI: Rust tests, clippy, Vitest, smoke E2E

---

## References

- `docs/milestones/M02-workspace-and-board.md`
- `docs/philosophy/final_agent_workspace_product_design.md` — §5 board, §6 agents, §9 comments, §19 UI
- `docs/philosophy/final_agent_workspace_framework_selection.md` — §5–6 frontend
- `docs/superpowers/specs/2026-06-07-coppice-milestone-strategy-design.md`
- M01 auth patterns: `server/src/api/auth.rs`, `server/src/middleware/`

## Next step

After user approves this spec, invoke the **writing-plans** skill to produce `docs/superpowers/plans/2026-06-07-m02-workspace-and-board.md` with task-by-task implementation steps. Frontend tasks must reference frontend-design skill as first sub-step.
