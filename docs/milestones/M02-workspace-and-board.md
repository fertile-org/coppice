# M02 — Workspace & Board

## Goal

Authenticated Coppice SPA with a Trello-like board, ticket and comment management, and agent configuration. Agents cannot run yet — this milestone establishes the collaborative workspace shell.

## Product scope

- React / Vite SPA with TanStack Query, Tailwind CSS, Radix UI / shadcn
- Login page; all routes except `/login` require valid session
- Projects and repos (minimal CRUD)
- Fixed board columns: Backlog, Ready, In Progress, In Review, In QA, Wait for Final Review, Done, Blocked
- Ticket CRUD: title, description, status, priority, assignee, repo/branch badges
- Drag-and-drop column moves (dnd-kit)
- Ticket detail tabs: Description, Comments, Metadata
- Comments: human, agent (placeholder author), system; markdown body; attachment metadata
- File upload for attachments (stored on server volume, metadata in Postgres)
- Agents CRUD with presets (PM, Tech Lead, FE, BE, DBA, QC, Reviewer, DevOps, Security, Research)
- Agent fields: role, skills, responsibilities, system prompt, provider config ref, enabled flag
- Manual ticket assignment to agent
- REST JSON API for all entities; server owns all state transitions (simple status update only — no workflow engine)

## Out of scope

- Agent runs and job queue (M03)
- Workflow rules and mention-driven jobs (M05)
- WebSocket realtime (M04)
- Knowledge, capabilities, secrets, signals
- Live Console tab

## Dependencies

- M01: session auth API, Postgres, server scaffold

## Architecture notes

### New server modules

```text
server/src/
  api/
    projects.rs
    repos.rs
    tickets.rs
    comments.rs
    agents.rs
    attachments.rs
  domain/
    project.rs, repo.rs, ticket.rs, comment.rs, agent.rs, attachment.rs
  services/
    ticket_service.rs, comment_service.rs, agent_service.rs
```

### New database tables

```text
projects
repos
agents
agent_presets (seed data)
tickets
ticket_comments
attachments
```

### API groups (M02)

```text
GET/POST       /api/projects
GET/PATCH      /api/projects/:id
GET/POST       /api/projects/:id/repos
GET/PATCH/DELETE /api/repos/:id

GET/POST       /api/tickets
GET/PATCH      /api/tickets/:id
PATCH          /api/tickets/:id/status
POST           /api/tickets/:id/assign

GET/POST       /api/tickets/:id/comments

GET/POST       /api/agents
GET/PATCH/DELETE /api/agents/:id

POST           /api/attachments  (multipart)
```

### Frontend features

```text
web/src/features/
  auth/          login page, session hook
  board/         kanban columns, dnd-kit
  tickets/       detail drawer/page, comment thread
  agents/        list + edit form (React Hook Form + Zod)
  projects/      minimal project picker
```

## Docker Compose delta

**Added in M02:**

```yaml
  web:
    build: ./web
    depends_on:
      - server
    ports:
      - "5173:5173"   # dev
    environment:
      VITE_API_URL: http://server:8080

  server:
    volumes:
      - artifact_data:/data/artifacts
    # optional: serve built SPA from server in production profile

volumes:
  artifact_data:
```

Decision at implementation time: separate `web` container for dev, or build SPA into server static — both must work with `docker compose up`.

## Testing strategy

### Unit tests

- **Server:** ticket status validation, comment intent enum parsing, agent preset seeding
- **Web:** Zod schemas for ticket/agent forms, board column ordering helper

### Integration tests

- Create project → repo → ticket → comment thread
- Drag status: PATCH status updates DB; invalid column rejected
- Assign agent to ticket; verify assigneeAgentId
- Attachment upload → file on volume + metadata row
- Agent CRUD with preset template
- All routes require session (401 without cookie)

### E2E smoke (CI, agent-browser)

Backend via `docker compose up`. Script in `e2e/smoke/m02-board.spec`:

1. Open login page → enter bootstrap credentials → land on board
2. Create ticket in Backlog
3. Drag ticket to Ready
4. Open ticket → add human comment → verify visible

### E2E full (local, `make e2e`)

- Create agent from PM preset
- Upload attachment on comment
- Filter/search tickets (if implemented; optional)
- Verify all board columns render

## Acceptance criteria

- [ ] Login gates SPA; session persists across refresh
- [ ] Full board CRUD works via UI and API
- [ ] Comments and attachments work on ticket detail
- [ ] Agents can be created from presets and assigned manually
- [ ] `docker compose up` yields working board with no extra setup
- [ ] CI smoke E2E passes

## References

- Product design §5 (board & tickets), §6 (agent management), §9 (comments), §12 (artifacts), §19 (UI overview)
- Framework selection §5–6 (frontend libraries)
