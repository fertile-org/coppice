# M01 Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the Coppice monorepo with a Docker Compose–backed Rust server (session auth, MockProvider), operator CLI, and CI test harness.

**Architecture:** Root Cargo workspace (`server/`, `cli/`). Axum HTTP API with SQLx + Postgres/pgvector. Session cookies + CSRF on mutations. `AgentProvider` trait with fixture-driven `MockProvider`. CLI wraps migrate/health/bootstrap against DB or HTTP.

**Tech Stack:** Rust 2021, Axum, Tokio, SQLx, Figment, Argon2, tracing, Docker Compose, pgvector/pg16, GitHub Actions

**Spec:** [docs/milestones/M01-foundation.md](../milestones/M01-foundation.md)

---

## File map (created in this milestone)

| Path | Responsibility |
|------|----------------|
| `Cargo.toml` | Workspace root |
| `Makefile` | compose-up, test, migrate, clippy |
| `README.md` | Monorepo map + quick start |
| `server/Cargo.toml` | Server deps |
| `server/src/main.rs` | Entry, router, startup |
| `server/src/config/mod.rs` | Figment config |
| `server/src/db/pool.rs` | PgPool + migrate on start |
| `server/src/domain/user.rs` | User model |
| `server/src/domain/session.rs` | Session model |
| `server/src/services/auth_service.rs` | Login, logout, bootstrap, password |
| `server/src/api/mod.rs` | Router composition |
| `server/src/api/health.rs` | GET /health |
| `server/src/api/auth.rs` | Auth handlers |
| `server/src/middleware/session.rs` | Extract session user |
| `server/src/middleware/csrf.rs` | CSRF validation |
| `server/src/providers/mod.rs` | AgentProvider trait + types |
| `server/src/providers/mock.rs` | MockProvider |
| `server/migrations/001_init.sql` | users, sessions, pgvector |
| `server/tests/integration_auth.rs` | Auth integration tests |
| `cli/Cargo.toml` | CLI deps |
| `cli/src/main.rs` | clap entry |
| `cli/src/commands/*.rs` | health, migrate, bootstrap |
| `deploy/docker-compose.yml` | postgres + server |
| `deploy/Dockerfile.server` | Multi-stage server image |
| `deploy/config/default.yaml` | Default server config |
| `fixtures/agent-responses/done.json` | Mock provider fixture |
| `web/README.md` | M02 placeholder |
| `.github/workflows/ci.yml` | CI pipeline |
| `.gitignore` | Rust, env, target |

---

### Task 1: Monorepo scaffold

**Files:**
- Create: `Cargo.toml`, `README.md`, `Makefile`, `.gitignore`, `web/README.md`, `e2e/smoke/.gitkeep`, `e2e/full/.gitkeep`, `fixtures/agent-responses/.gitkeep`

- [ ] **Step 1: Create workspace `Cargo.toml`**

```toml
[workspace]
members = ["server", "cli"]
resolver = "2"

[workspace.package]
edition = "2021"
license = "MIT OR Apache-2.0"
```

- [ ] **Step 2: Create `.gitignore`**

```gitignore
/target/
**/*.rs.bk
.env
.env.*
!.env.example
/server/.sqlx/
```

- [ ] **Step 3: Create root `README.md`**

Include Coppice slogan, monorepo folder table (`server/`, `web/`, `cli/`, `deploy/`, `docs/`, `e2e/`, `fixtures/`), and quick start:

```bash
cp deploy/.env.example .env
make compose-up
make migrate
coppice bootstrap admin --email admin@localhost --password changeme
curl http://localhost:8080/health
```

- [ ] **Step 4: Create `Makefile`**

```makefile
COMPOSE = docker compose -f deploy/docker-compose.yml

.PHONY: compose-up compose-down test clippy migrate

compose-up:
	$(COMPOSE) up -d --build

compose-down:
	$(COMPOSE) down

migrate:
	cargo run -p coppice-cli -- migrate

test:
	cargo test --workspace

clippy:
	cargo clippy --workspace -- -D warnings
```

- [ ] **Step 5: Create `web/README.md`**

```markdown
# Coppice Web

React SPA — implemented in milestone M02.
```

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml README.md Makefile .gitignore web/README.md e2e fixtures
git commit -m "chore: add Coppice monorepo scaffold"
```

---

### Task 2: Server crate skeleton + health endpoint

**Files:**
- Create: `server/Cargo.toml`, `server/src/main.rs`, `server/src/api/mod.rs`, `server/src/api/health.rs`

- [ ] **Step 1: Write failing health integration test**

Create `server/tests/health.rs`:

```rust
use axum::{body::Body, http::{Request, StatusCode}};
use tower::ServiceExt;

#[tokio::test]
async fn health_returns_ok() {
    let app = coppice_server::app(coppice_server::test_state().await);
    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
```

This requires exposing `app()` and `test_state()` from the library — use `server/src/lib.rs`.

- [ ] **Step 2: Run test — expect fail**

Run: `cargo test -p coppice-server --test health`
Expected: FAIL (crate not found)

- [ ] **Step 3: Create `server/Cargo.toml`**

```toml
[package]
name = "coppice-server"
version = "0.1.0"
edition = "2021"

[lib]
name = "coppice_server"
path = "src/lib.rs"

[[bin]]
name = "coppice-server"
path = "src/main.rs"

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["trace", "cors"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
anyhow = "1"

[dev-dependencies]
http = "1"
http-body-util = "0.1"
```

- [ ] **Step 4: Create `server/src/lib.rs`**

```rust
pub mod api;

use axum::Router;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    // extended in later tasks
}

pub async fn test_state() -> Arc<AppState> {
    Arc::new(AppState {})
}

pub fn app(state: Arc<AppState>) -> Router {
    api::router(state)
}
```

- [ ] **Step 5: Create `server/src/api/mod.rs` and `health.rs`**

```rust
// api/mod.rs
mod health;

use axum::Router;
use std::sync::Arc;
use crate::AppState;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(health::routes())
        .with_state(state)
}
```

```rust
// api/health.rs
use axum::{routing::get, Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/health", get(health))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "coppice-server" }))
}
```

- [ ] **Step 6: Create `server/src/main.rs`**

```rust
use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let state = Arc::new(coppice_server::AppState {});
    let app = coppice_server::app(state);
    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    tracing::info!(%addr, "listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 7: Run test — expect pass**

Run: `cargo test -p coppice-server --test health`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add server/
git commit -m "feat(server): add axum skeleton and health endpoint"
```

---

### Task 3: Config loading (figment)

**Files:**
- Create: `server/src/config/mod.rs`, `deploy/config/default.yaml`, `deploy/.env.example`
- Modify: `server/src/lib.rs`, `server/src/main.rs`, `server/Cargo.toml`

- [ ] **Step 1: Write failing config unit test**

Create `server/src/config/mod.rs` with `#[cfg(test)] mod tests`:

```rust
#[test]
fn loads_defaults_without_file() {
    let cfg = AppConfig::load(None).expect("defaults");
    assert_eq!(cfg.server.port, 8080);
}
```

- [ ] **Step 2: Run test — expect fail**

Run: `cargo test -p coppice-server config::tests::loads_defaults_without_file`
Expected: FAIL

- [ ] **Step 3: Add figment + implement `AppConfig`**

Add to `server/Cargo.toml`: `figment = { version = "0.10", features = ["yaml", "env"] }`

```rust
use figment::{Figment, providers::{Env, Format, Yaml, Serialized}};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub session_secret: String,
    pub bootstrap_password: String,
    pub cookie_secure: bool,
}

impl AppConfig {
    pub fn load(config_path: Option<&str>) -> Result<Self, figment::Error> {
        let mut figment = Figment::new()
            .merge(Serialized::defaults(AppConfig::default_values()))
            .merge(Env::prefixed("COPPICE_").split("_"));

        if let Some(path) = config_path {
            figment = figment.merge(Yaml::file(path));
        }

        figment.extract()
    }

    fn default_values() -> Self {
        Self {
            server: ServerConfig { port: 8080 },
            database: DatabaseConfig {
                url: "postgres://coppice:coppice@localhost:5432/coppice".into(),
            },
            auth: AuthConfig {
                session_secret: "dev-secret-change-me".into(),
                bootstrap_password: "changeme".into(),
                cookie_secure: false,
            },
        }
    }
}
```

Create `deploy/config/default.yaml` mirroring defaults. Create `deploy/.env.example` with `DATABASE_URL`, `SESSION_SECRET`, `COPPICE_BOOTSTRAP_PASSWORD`.

Wire `main.rs` to load config from `COPPICE_CONFIG` env or `deploy/config/default.yaml`.

- [ ] **Step 4: Run test — expect pass**

Run: `cargo test -p coppice-server config`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add server deploy/config deploy/.env.example
git commit -m "feat(server): add figment config loading"
```

---

### Task 4: Database pool + migrations (users, sessions, pgvector)

**Files:**
- Create: `server/migrations/001_init.sql`, `server/src/db/pool.rs`, `server/src/db/mod.rs`
- Modify: `server/Cargo.toml`, `server/src/lib.rs`, `AppState`

- [ ] **Step 1: Write migration SQL**

`server/migrations/001_init.sql`:

```sql
CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE users (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'admin',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    csrf_token TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX sessions_user_id_idx ON sessions(user_id);
CREATE INDEX sessions_expires_at_idx ON sessions(expires_at);
```

- [ ] **Step 2: Add sqlx deps**

```toml
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "time", "migrate"] }
uuid = { version = "1", features = ["v4", "serde"] }
time = { version = "0.3", features = ["serde"] }
```

- [ ] **Step 3: Implement `db/pool.rs`**

```rust
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub async fn connect_and_migrate(database_url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}
```

Store `PgPool` in `AppState`.

- [ ] **Step 4: Create `deploy/docker-compose.yml`**

Use compose file from M01 spec (postgres pgvector + server), with `DATABASE_URL` env.

- [ ] **Step 5: Verify migrations manually**

Run:

```bash
docker compose -f deploy/docker-compose.yml up -d postgres
DATABASE_URL=postgres://coppice:coppice@localhost:5432/coppice cargo sqlx migrate run --source server/migrations
```

Expected: migrations apply; `\dx` shows vector extension.

- [ ] **Step 6: Commit**

```bash
git add server/migrations server/src/db deploy/docker-compose.yml
git commit -m "feat(server): add postgres migrations with pgvector"
```

---

### Task 5: Auth domain + password hashing

**Files:**
- Create: `server/src/domain/mod.rs`, `server/src/domain/user.rs`, `server/src/services/mod.rs`, `server/src/services/auth_service.rs`
- Modify: `server/Cargo.toml` (add `argon2`, `rand`)

- [ ] **Step 1: Write failing unit tests for password hash**

In `server/src/services/auth_service.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_password() {
        let hash = hash_password("secret").unwrap();
        assert!(verify_password("secret", &hash).unwrap());
        assert!(!verify_password("wrong", &hash).unwrap());
    }
}
```

- [ ] **Step 2: Run test — expect fail**

Run: `cargo test -p coppice-server hash_and_verify_password`
Expected: FAIL

- [ ] **Step 3: Implement argon2 helpers + User repository methods**

Use `argon2` crate with OsRng salt. Add `User` struct matching DB row. Add `AuthService` with:

- `bootstrap_admin(email, password) -> User` (only if user count = 0)
- `login(email, password) -> SessionBundle` (session row + cookies)
- `logout(session_id)`
- `user_by_session(token) -> User`

Session token: 32 random bytes hex; store SHA-256 hash in DB. CSRF token: separate random 32 bytes stored in session row.

- [ ] **Step 4: Run test — expect pass**

Run: `cargo test -p coppice-server auth_service`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add server/src/domain server/src/services
git commit -m "feat(server): add auth service with argon2"
```

---

### Task 6: Auth HTTP API + cookies

**Files:**
- Create: `server/src/api/auth.rs`
- Modify: `server/src/api/mod.rs`

- [ ] **Step 1: Write integration test skeleton**

Create `server/tests/integration_auth.rs`:

```rust
#[tokio::test]
async fn bootstrap_login_me_logout_flow() {
    let state = test_state_with_db().await;
    let app = coppice_server::app(state);

    // bootstrap
    // login -> capture Set-Cookie
    // GET /api/auth/me with cookie -> 200
    // POST /api/auth/logout with cookie + csrf -> 200
    // GET /api/auth/me -> 401
}
```

Helper `test_state_with_db()` reads `DATABASE_URL` from env (CI sets it to compose postgres).

- [ ] **Step 2: Implement auth routes**

```text
POST /api/auth/bootstrap   { email, password }  + header X-Bootstrap-Password
POST /api/auth/login       { email, password }
POST /api/auth/logout      (session + CSRF)
GET  /api/auth/me          (session)
```

Response cookies:

- `coppice_session` — httpOnly, SameSite=Lax, Path=/, Secure from config
- Login JSON returns `{ user, csrfToken }` for future SPA

Bootstrap returns 403 if any user exists or bootstrap password wrong.

- [ ] **Step 3: Run integration test against compose**

```bash
make compose-up
DATABASE_URL=postgres://coppice:coppice@localhost:5432/coppice \
  cargo test -p coppice-server --test integration_auth
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add server/src/api/auth.rs server/tests/integration_auth.rs
git commit -m "feat(server): add session auth API"
```

---

### Task 7: Session + CSRF middleware

**Files:**
- Create: `server/src/middleware/mod.rs`, `server/src/middleware/session.rs`, `server/src/middleware/csrf.rs`
- Modify: `server/src/api/mod.rs`

- [ ] **Step 1: Write failing integration test for CSRF**

Add to `integration_auth.rs`:

```rust
#[tokio::test]
async fn logout_without_csrf_is_forbidden() {
    // login, then POST /api/auth/logout without X-CSRF-Token
    // expect 403
}
```

- [ ] **Step 2: Implement middleware**

- `session_middleware`: parse `coppice_session` cookie → load user → insert `AuthUser` into request extensions
- `csrf_middleware`: for POST/PUT/PATCH/DELETE under `/api/`, require `X-CSRF-Token` header matching session's csrf_token
- Public routes: `/health`, `/api/auth/login`, `/api/auth/bootstrap`

- [ ] **Step 3: Add test unauthenticated `/api/auth/me` returns 401**

- [ ] **Step 4: Run integration tests — expect pass**

- [ ] **Step 5: Commit**

```bash
git add server/src/middleware server/tests/integration_auth.rs
git commit -m "feat(server): add session and CSRF middleware"
```

---

### Task 8: AgentProvider trait + MockProvider

**Files:**
- Create: `server/src/providers/mod.rs`, `server/src/providers/mock.rs`, `fixtures/agent-responses/done.json`, `fixtures/agent-responses/blocked.json`
- Modify: `server/Cargo.toml` (add `async-trait`)

- [ ] **Step 1: Write failing unit tests**

`fixtures/agent-responses/done.json`:

```json
{
  "status": "done",
  "summary": "Mock implementation complete.",
  "changedFiles": [],
  "testsRun": [],
  "nextStatus": "In Review",
  "mentionAgents": [],
  "blockers": []
}
```

Tests:
- `agent_run_result_deserializes_done_fixture`
- `mock_provider_returns_done_fixture`

- [ ] **Step 2: Define types in `providers/mod.rs`**

```rust
#[async_trait::async_trait]
pub trait AgentProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn run(&self, input: AgentRunInput) -> Result<AgentRunResult, ProviderError>;
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentRunInput {
    pub agent_id: String,
    pub ticket_id: Option<String>,
    pub context_path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgentRunResult {
    Done { summary: String, changed_files: Vec<String>, tests_run: Vec<String>, next_status: String, mention_agents: Vec<String>, blockers: Vec<String> },
    Blocked { blocker_type: String, summary: String, next_status: String, mention_agents: Vec<String> },
    // add variants matching product design §17 as needed for tests
}
```

Use `#[serde(rename_all = "camelCase")]` on struct fields to match product JSON.

- [ ] **Step 3: Implement `MockProvider`**

Reads `fixtures/agent-responses/{response_name}.json` defaulting to `done.json`. Ignores `AgentRunInput` content except optional env override `MOCK_AGENT_RESPONSE`.

- [ ] **Step 4: Run unit tests — expect pass**

Run: `cargo test -p coppice-server providers`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add server/src/providers fixtures/
git commit -m "feat(server): add AgentProvider trait and MockProvider"
```

---

### Task 9: CLI crate (`coppice`)

**Files:**
- Create: `cli/Cargo.toml`, `cli/src/main.rs`, `cli/src/commands/mod.rs`, `cli/src/commands/health.rs`, `cli/src/commands/migrate.rs`, `cli/src/commands/bootstrap.rs`

- [ ] **Step 1: Create `cli/Cargo.toml`**

```toml
[package]
name = "coppice-cli"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "coppice"
path = "src/main.rs"

[dependencies]
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "migrate"] }
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: Implement clap commands**

```rust
// main.rs
#[derive(clap::Parser)]
#[command(name = "coppice", about = "Coppice operator CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    Health(HealthArgs),
    Migrate(MigrateArgs),
    Bootstrap(BootstrapArgs),
}

#[derive(clap::Args)]
struct BootstrapArgs {
    #[arg(long)]
    email: String,
    #[arg(long)]
    password: String,
    #[arg(long, env = "COPPICE_SERVER_URL", default_value = "http://localhost:8080")]
    server_url: String,
    #[arg(long, env = "COPPICE_BOOTSTRAP_PASSWORD", default_value = "changeme")]
    bootstrap_password: String,
}
```

- `health`: GET `{server_url}/health`, print OK/ FAIL; optionally check postgres with `DATABASE_URL`
- `migrate`: run `sqlx migrate run` against `DATABASE_URL` using migrations path `server/migrations` (use `Migrator::new` with path relative to repo root — document running from repo root)
- `bootstrap admin`: POST `{server_url}/api/auth/bootstrap` with JSON body + `X-Bootstrap-Password` header

- [ ] **Step 3: Manual smoke test**

```bash
make compose-up
make migrate
cargo run -p coppice-cli -- bootstrap admin --email admin@localhost --password changeme
cargo run -p coppice-cli -- health
```

Expected: bootstrap creates admin; health prints ok.

- [ ] **Step 4: Commit**

```bash
git add cli/
git commit -m "feat(cli): add coppice operator commands"
```

---

### Task 10: Server Dockerfile + compose server service

**Files:**
- Create: `deploy/Dockerfile.server`
- Modify: `deploy/docker-compose.yml`, `server/src/main.rs` (bind port from config)

- [ ] **Step 1: Create multi-stage `deploy/Dockerfile.server`**

```dockerfile
FROM rust:1.83-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY server ./server
COPY cli ./cli
RUN cargo build --release -p coppice-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/coppice-server /usr/local/bin/coppice-server
COPY deploy/config/default.yaml /etc/coppice/config.yaml
ENV COPPICE_CONFIG=/etc/coppice/config.yaml
EXPOSE 8080
CMD ["coppice-server"]
```

- [ ] **Step 2: Wire server to run migrations on startup**

In `main.rs`: connect pool via `connect_and_migrate`, pass pool + config into `AppState`.

- [ ] **Step 3: Verify full compose stack**

```bash
docker compose -f deploy/docker-compose.yml up --build -d
curl -s http://localhost:8080/health
```

Expected: `{"status":"ok","service":"coppice-server"}`

- [ ] **Step 4: Commit**

```bash
git add deploy/Dockerfile.server server/src/main.rs
git commit -m "feat(deploy): add server Dockerfile and compose wiring"
```

---

### Task 11: CI pipeline

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Add GitHub Actions workflow**

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  rust:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: pgvector/pgvector:pg16
        env:
          POSTGRES_USER: coppice
          POSTGRES_PASSWORD: coppice
          POSTGRES_DB: coppice
        ports:
          - 5432:5432
        options: >-
          --health-cmd "pg_isready -U coppice"
          --health-interval 5s
          --health-timeout 5s
          --health-retries 10

    env:
      DATABASE_URL: postgres://coppice:coppice@localhost:5432/coppice
      SESSION_SECRET: ci-test-secret
      COPPICE_BOOTSTRAP_PASSWORD: changeme

    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Migrate
        run: cargo run -p coppice-cli -- migrate
      - name: Test
        run: cargo test --workspace
      - name: Clippy
        run: cargo clippy --workspace -- -D warnings
```

- [ ] **Step 2: Push branch and verify CI green**

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add rust test and clippy workflow"
```

---

### Task 12: Milestone acceptance verification

- [ ] **Step 1: Run full checklist from M01 spec**

```bash
make compose-up
make migrate
cargo run -p coppice-cli -- bootstrap admin --email admin@localhost --password changeme
cargo run -p coppice-cli -- health
cargo test --workspace
make clippy
```

- [ ] **Step 2: Verify monorepo folders exist**

Confirm: `server/`, `web/`, `cli/`, `deploy/`, `docs/`, `e2e/`, `fixtures/`, root `Cargo.toml`, `Makefile`, `README.md`

- [ ] **Step 3: Final commit if any fixes**

```bash
git commit -m "chore(m01): complete foundation milestone acceptance"
```

---

## Spec coverage self-review

| M01 requirement | Task |
|-----------------|------|
| Monorepo scaffold | Task 1 |
| Session auth API | Tasks 5–7 |
| argon2 + CSRF | Tasks 5, 7 |
| Bootstrap via CLI | Tasks 6, 9 |
| Figment config | Task 3 |
| Health endpoint | Task 2 |
| AgentProvider + MockProvider | Task 8 |
| pgvector migration | Task 4 |
| docker compose | Tasks 4, 10 |
| CLI health/migrate/bootstrap | Task 9 |
| CI cargo test + clippy | Task 11 |
| web/ placeholder | Task 1 |

No gaps identified.

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-07-m01-foundation.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — Fresh subagent per task, review between tasks, fast iteration  
2. **Inline Execution** — Execute tasks in this session with checkpoints

Which approach?
