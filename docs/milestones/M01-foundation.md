# M01 — Foundation

## Goal

Runnable Coppice backend with PostgreSQL (pgvector-ready), session auth API, the `AgentProvider` trait with `MockProvider`, and a CI test harness. No product UI.

## Product scope

- Rust / Axum / Tokio server scaffold
- SQLx migrations and repository layer patterns
- PostgreSQL 16 with pgvector extension enabled (embeddings used in M06)
- Session auth API: login, logout, `/me`
- Password hashing with argon2; httpOnly secure session cookie; CSRF on mutating requests
- Bootstrap first admin user via `COPPICE_BOOTSTRAP_PASSWORD` env or `scripts/bootstrap-admin.sh`
- Config loading via figment (YAML + environment variables)
- Structured logging with `tracing`
- Health endpoint (`GET /health`) — unauthenticated
- `AgentProvider` trait and `MockProvider` adapter (returns dummy `AgentRunResult` JSON)
- `deploy/docker-compose.yml` with `postgres` and `server` services
- CI pipeline: `cargo test`, `cargo clippy`, integration tests against compose Postgres

## Out of scope

- React SPA and login UI (M02)
- Board, tickets, agents, workflow
- WebSocket endpoints
- Real CLI provider adapters (Claude Code, Codex, etc.)
- Worktrees, job queue, artifacts

## Dependencies

- None (first milestone)
- Prior milestones: —

## Architecture notes

### Server modules (initial)

```text
server/src/
  main.rs
  config/
  api/
    health.rs
    auth.rs
  domain/
    user.rs
    session.rs
  services/
    auth_service.rs
  providers/
    mod.rs          # AgentProvider trait
    mock.rs         # MockProvider
  db/
    pool.rs
  middleware/
    session.rs
    csrf.rs
```

### Database tables (M01)

```text
users
sessions
schema_migrations (sqlx)
```

### API endpoints (M01)

```text
GET  /health
POST /api/auth/login
POST /api/auth/logout
GET  /api/auth/me
```

### AgentProvider trait

```rust
#[async_trait]
pub trait AgentProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn run(&self, input: AgentRunInput) -> Result<AgentRunResult, ProviderError>;
}
```

`MockProvider` ignores prompt details and returns configurable fixture JSON from `fixtures/agent-responses/`.

## Docker Compose delta

**New in M01:**

```yaml
services:
  postgres:
    image: pgvector/pgvector:pg16
    environment:
      POSTGRES_DB: coppice
      POSTGRES_USER: coppice
      POSTGRES_PASSWORD: coppice
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U coppice"]

  server:
    build: ./server
    depends_on:
      postgres:
        condition: service_healthy
    environment:
      DATABASE_URL: postgres://coppice:coppice@postgres:5432/coppice
      COPPICE_BOOTSTRAP_PASSWORD: ${COPPICE_BOOTSTRAP_PASSWORD:-changeme}
      SESSION_SECRET: ${SESSION_SECRET:-dev-secret-change-me}
    ports:
      - "8080:8080"

volumes:
  postgres_data:
```

## Testing strategy

### Unit tests

- Config parsing (figment): defaults, env overrides, invalid config errors
- Password hash and verify (argon2)
- Session token generation and validation
- `AgentRunResult` JSON deserialization (all status variants from product design §17)
- `MockProvider` returns expected fixture output

### Integration tests

- Run against compose Postgres (test harness starts migrations)
- Login → receive Set-Cookie → `/me` returns user → logout invalidates session
- Unauthenticated `/api/*` returns 401 (except health and login)
- CSRF: mutating request without token returns 403
- Bootstrap: first login with bootstrap password creates admin user

### E2E smoke (CI)

Not applicable for M01.

### E2E full (local)

Not applicable for M01.

### CI job shape

```text
1. docker compose up -d postgres
2. cargo sqlx migrate run (or server auto-migrate on start)
3. cargo test
4. cargo test --test integration_*
5. cargo clippy -- -D warnings
```

## Acceptance criteria

- [ ] `docker compose up` starts postgres and server without errors
- [ ] `GET /health` returns 200
- [ ] Admin bootstrap works; login returns httpOnly session cookie
- [ ] Authenticated `/me` returns user; logout clears session
- [ ] `MockProvider` trait exists and passes unit tests
- [ ] pgvector extension is installed in Postgres (`CREATE EXTENSION vector` in migration)
- [ ] CI pipeline passes on a clean checkout

## References

- Product design §20 (server responsibilities), §22 (API overview), §23 (data storage)
- Framework selection §1–3, §9–10
- Milestone strategy: auth model, provider adapter, compose conventions
