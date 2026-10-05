# Prefer Docker Compose v2 plugin; fall back to docker-compose standalone (Homebrew).
ifeq ($(shell docker compose version >/dev/null 2>&1 && echo yes),yes)
DOCKER_COMPOSE = docker compose
else
DOCKER_COMPOSE = docker-compose
endif

COMPOSE = $(DOCKER_COMPOSE) -f deploy/docker-compose.yml
COMPOSE_LOCAL = $(DOCKER_COMPOSE) -f deploy/docker-compose.local.yml
BOOTSTRAP_EMAIL = admin@localhost
BOOTSTRAP_PASSWORD = changeme
# Match host user so bind-mounted /repos files stay owned by the operator.
export COPPICE_UID ?= $(shell id -u)
export COPPICE_GID ?= $(shell id -g)

.PHONY: compose-up compose-down compose-local-up compose-local-down server server-dev tools test test-unit test-smoke test-pg-reset clippy clean migrate bootstrap web-install web-test web-dev web-build website-install website-dev website-build desktop desktop-test desktop-dist-dir desktop-smoke e2e-smoke e2e-smoke-m03 e2e-smoke-m04 e2e-smoke-m05 e2e-smoke-m06 e2e-smoke-m06-knowledge e2e-smoke-m09 e2e-smoke-m10 screenshot benchmark-m06-knowledge-retrieval release-tar

CARGO_TEST = cargo test --features embedded-test-db

# Create /tmp/smoke-repo inside the server container as COPPICE_UID (matches API process).
define SMOKE_REPO_SETUP
	$(COMPOSE) exec -T server sh -c 'rm -rf /tmp/smoke-repo && mkdir -p /tmp/smoke-repo && chown $(COPPICE_UID):$(COPPICE_GID) /tmp/smoke-repo'
	$(COMPOSE) exec -T -u $(COPPICE_UID):$(COPPICE_GID) server sh -c 'cd /tmp/smoke-repo && git init -b main && git config user.email smoke@coppice.local && git config user.name smoke && echo hi > README.md && git add . && git commit -m init'
endef

define SMOKE_REPO_SETUP_IF_MISSING
	$(COMPOSE) exec -T server sh -c 'if [ ! -d /tmp/smoke-repo/.git ]; then mkdir -p /tmp/smoke-repo && chown $(COPPICE_UID):$(COPPICE_GID) /tmp/smoke-repo; fi'
	$(COMPOSE) exec -T -u $(COPPICE_UID):$(COPPICE_GID) server sh -c 'if [ ! -d /tmp/smoke-repo/.git ]; then cd /tmp/smoke-repo && git init -b main && git config user.email smoke@coppice.local && git config user.name smoke && echo hi > README.md && git add . && git commit -m init; fi'
endef

compose-up:
	@test -f deploy/config/config.toml || cp deploy/config/config.example.toml deploy/config/config.toml
	$(COMPOSE) up -d --build

compose-down:
	$(COMPOSE) down

compose-local-up:
	$(COMPOSE_LOCAL) up -d

compose-local-down:
	$(COMPOSE_LOCAL) down

server:
	cargo run -p coppice-server --features mock-provider

server-dev:
	@command -v cargo-watch >/dev/null 2>&1 || { \
		echo "cargo-watch is required for API hot reload. Install with: cargo install cargo-watch"; \
		exit 1; \
	}
	cargo watch -q -c -x 'run -p coppice-server --features mock-provider'

migrate:
	cargo run -p coppice-cli -- migrate

bootstrap:
	cargo run -p coppice-cli -- bootstrap admin --email $(BOOTSTRAP_EMAIL) --password $(BOOTSTRAP_PASSWORD)

# Cargo dev tools: nextest (parallel tests), cargo-watch (server-dev).
tools:
	cargo install --locked cargo-nextest cargo-watch

# Full suite in parallel via cargo-nextest (one process and database per test, ~1 min warm).
# Falls back to the serial cargo test runner when nextest is not installed.
test:
	@if cargo nextest --version >/dev/null 2>&1; then \
		cargo nextest run --features embedded-test-db --workspace; \
	else \
		echo "cargo-nextest not found; running serially (install: https://nexte.st)"; \
		$(CARGO_TEST) --workspace -- --test-threads 1; \
	fi

# Unit tests only — isolated databases allow default parallelism (~5–15s warm).
test-unit:
	$(CARGO_TEST) --workspace --lib -q

# Smoke integration — lib + health + comments + tickets (~target <60s warm).
test-smoke:
	@if cargo nextest --version >/dev/null 2>&1; then \
		cargo nextest run --features embedded-test-db --workspace --lib && \
		cargo nextest run --features embedded-test-db -p coppice-server --test health --test integration_comments --test integration_tickets; \
	else \
		$(CARGO_TEST) --workspace --lib -q -- --test-threads 1 && \
		$(CARGO_TEST) -p coppice-server --test health --test integration_comments --test integration_tickets -q; \
	fi

# Drop session file so the next test run starts a fresh embedded Postgres (old process may linger).
test-pg-reset:
	rm -f $(HOME)/.cache/coppice/test-pg/session.json

clippy:
	cargo clippy --workspace -- -D warnings
	cargo clippy --workspace --features mock-provider -- -D warnings

clean:
	cargo clean

web-install:
	cd web && yarn install --frozen-lockfile

web-test:
	cd web && yarn install --frozen-lockfile && yarn test

web-dev:
	cd web && yarn install && yarn dev

web-build:
	cd web && yarn install --frozen-lockfile && yarn build

# Electron shell — requires Path A or B (web on :5001) already running.
desktop:
	cd desktop && yarn install && yarn start

desktop-test:
	cd desktop && yarn install --frozen-lockfile && yarn test

# POSTGRES_DIR=<dir with bin/lib/share> skips the pinned download (e.g. a pg-embed cache).
DESKTOP_PG_DIR = $(abspath $(or $(POSTGRES_DIR),desktop/.cache/postgres-host))

# Release binary without mock-provider: assemble-resources rejects a binary that still contains it.
desktop-dist-dir:
	cargo build --release --locked -p coppice-server
	cd web && yarn install --frozen-lockfile && yarn build
	cd desktop && yarn install --frozen-lockfile
ifndef POSTGRES_DIR
	cd desktop && node scripts/fetch-postgres.mjs --out $(DESKTOP_PG_DIR)
endif
	cd desktop && node scripts/assemble-resources.mjs --server-bin ../target/release/coppice-server --web-dist ../web/dist --postgres $(DESKTOP_PG_DIR)
	cd desktop && yarn dist:dir

# Resolved when the recipe runs so `make desktop-dist-dir desktop-smoke` works in one go.
desktop-smoke:
	@resources=$$(ls -d desktop/dist/linux*-unpacked/resources desktop/dist/mac*/Coppice.app/Contents/Resources 2>/dev/null | head -n 1); \
	test -n "$$resources" || { echo "no packaged build under desktop/dist; run make desktop-dist-dir" >&2; exit 1; }; \
	cd desktop && node scripts/headless-smoke.mjs --resources "$(CURDIR)/$$resources"

e2e-smoke:
	$(MAKE) compose-up
	node e2e/smoke/m02-board.mjs

e2e-smoke-m03:
	$(MAKE) compose-up
	MOCK_AGENT_RESPONSE=done WORKFLOW_AUTO_START_RUNS=false $(COMPOSE) up -d --force-recreate --no-deps server
	$(SMOKE_REPO_SETUP)
	node e2e/smoke/m03-agent-run.mjs

e2e-smoke-m04:
	$(MAKE) compose-up
	MOCK_AGENT_RESPONSE=done WORKFLOW_AUTO_START_RUNS=false $(COMPOSE) up -d --force-recreate --no-deps server
	$(SMOKE_REPO_SETUP)
	node e2e/smoke/m04-live-console.mjs

e2e-smoke-m05:
	$(MAKE) compose-up
	$(SMOKE_REPO_SETUP)
	node e2e/smoke/m05-workflow.mjs

e2e-smoke-m06:
	$(MAKE) compose-up
	WORKFLOW_AUTO_START_RUNS=false MOCK_AGENT_RESPONSE=pm/split_pending $(COMPOSE) up -d --force-recreate --no-deps server
	$(SMOKE_REPO_SETUP_IF_MISSING)
	node e2e/smoke/m06-context.mjs

e2e-smoke-m06-knowledge:
	$(MAKE) compose-up
	MOCK_AGENT_RESPONSE= $(COMPOSE) up -d --force-recreate --no-deps server
	$(SMOKE_REPO_SETUP_IF_MISSING)
	node e2e/smoke/m06-knowledge.mjs

e2e-smoke-m09:
	$(MAKE) compose-up
	MOCK_AGENT_RESPONSE=backend_engineer/chat_turn $(COMPOSE) up -d --force-recreate --no-deps server
	node e2e/smoke/m09-chat.mjs

e2e-smoke-m10:
	$(MAKE) compose-up
	MOCK_AGENT_RESPONSE=mcp/m10_smoke WORKFLOW_AUTO_START_RUNS=false $(COMPOSE) up -d --force-recreate --no-deps server
	$(SMOKE_REPO_SETUP_IF_MISSING)
	node e2e/smoke/m10-plugins.mjs

# Marketing board PNG for static/screenshot.png and the site hero. Not part of CI.
# Overlay forces auth.desktop_mode and disables auto-start so the shot matches
# the Electron app (no login, no account email, no Sign out).
screenshot:
	@test -f deploy/config/config.toml || cp deploy/config/config.example.toml deploy/config/config.toml
	$(COMPOSE) -f deploy/docker-compose.screenshot.yml up -d --build
	$(COMPOSE) -f deploy/docker-compose.screenshot.yml up -d --force-recreate --no-deps server
	cd e2e && yarn install --frozen-lockfile
	cd e2e && yarn playwright install chromium
	node e2e/screenshot/capture.mjs

benchmark-m06-knowledge-retrieval:
	$(MAKE) compose-up
	COPPICE_RETRIEVAL_BENCHMARK_DATABASE_URL=postgres://coppice:coppice@127.0.0.1:$${COPPICE_PG_PORT:-5432}/coppice \
		cargo test -p coppice-server --features embedded-test-db --test integration_knowledge knowledge_retrieval_capacity_p95_benchmark -- --ignored --nocapture --test-threads 1

website-install:
	cd website && npm install

website-dev:
	cd website && npm run dev

website-build:
	cd website && npm run build

release-tar: web-build
	cargo build --release -p coppice-server -p coppice-cli
	mkdir -p dist/release/web
	cp target/release/coppice-server dist/release/
	cp target/release/coppice dist/release/coppice-cli
	cp -r web/dist dist/release/web/dist
	cp config.example.toml dist/release/config.example.toml
	cp -r deploy/systemd dist/release/systemd
	tar -czf dist/coppice-$$(uname -s | tr A-Z a-z)-$$(uname -m).tar.gz -C dist/release .
