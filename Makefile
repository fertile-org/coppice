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

.PHONY: compose-up compose-down compose-local-up compose-local-down server server-dev test test-unit test-smoke test-pg-reset clippy clean migrate bootstrap web-install web-test web-dev web-build e2e-smoke e2e-smoke-m03 e2e-smoke-m04 e2e-smoke-m05 e2e-smoke-m06 e2e-smoke-m06-knowledge release-tar

CARGO_TEST = cargo test --features embedded-test-db

compose-up:
	$(COMPOSE) up -d --build

compose-down:
	$(COMPOSE) down

compose-local-up:
	$(COMPOSE_LOCAL) up -d

compose-local-down:
	$(COMPOSE_LOCAL) down

server:
	cargo run -p coppice-server

server-dev:
	@command -v cargo-watch >/dev/null 2>&1 || { \
		echo "cargo-watch is required for API hot reload. Install with: cargo install cargo-watch"; \
		exit 1; \
	}
	cargo watch -q -c -x 'run -p coppice-server'

migrate:
	cargo run -p coppice-cli -- migrate

bootstrap:
	cargo run -p coppice-cli -- bootstrap admin --email $(BOOTSTRAP_EMAIL) --password $(BOOTSTRAP_PASSWORD)

# Full suite — one shared embedded Postgres for all binaries (~2–3 min warm).
test:
	$(CARGO_TEST) --workspace -- --test-threads 1

# Unit tests only — lib crates, no integration binaries (~5–15s warm).
test-unit:
	$(CARGO_TEST) --workspace --lib -q -- --test-threads 1

# Smoke integration — lib + health + comments + tickets (~target <60s warm).
test-smoke:
	$(CARGO_TEST) --workspace --lib -q -- --test-threads 1
	$(CARGO_TEST) -p coppice-server --test health --test integration_comments --test integration_tickets -q

# Drop session file so the next test run starts a fresh embedded Postgres (old process may linger).
test-pg-reset:
	rm -f $(HOME)/.cache/coppice/test-pg/session.json

clippy:
	cargo clippy --workspace -- -D warnings

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

e2e-smoke: compose-up
	node e2e/smoke/m02-board.mjs

e2e-smoke-m03: compose-up
	$(COMPOSE) exec -T server sh -c 'mkdir -p /tmp/smoke-repo && cd /tmp/smoke-repo && git init -b main && git config user.email smoke@coppice.local && git config user.name smoke && echo hi > README.md && git add . && git commit -m init'
	node e2e/smoke/m03-agent-run.mjs

e2e-smoke-m04: compose-up
	$(COMPOSE) exec -T server sh -c 'mkdir -p /tmp/smoke-repo && cd /tmp/smoke-repo && git init -b main && git config user.email smoke@coppice.local && git config user.name smoke && echo hi > README.md && git add . && git commit -m init'
	node e2e/smoke/m04-live-console.mjs

e2e-smoke-m05: compose-up
	$(COMPOSE) exec -T server sh -c 'mkdir -p /tmp/smoke-repo && cd /tmp/smoke-repo && git init -b main && git config user.email smoke@coppice.local && git config user.name smoke && echo hi > README.md && git add . && git commit -m init'
	node e2e/smoke/m05-workflow.mjs

e2e-smoke-m06: compose-up
	$(COMPOSE) exec -T server sh -c 'mkdir -p /tmp/smoke-repo && cd /tmp/smoke-repo && git init -b main && git config user.email smoke@coppice.local && git config user.name smoke && echo hi > README.md && git add . && git commit -m init'
	MOCK_AGENT_RESPONSE=pm/split_pending $(COMPOSE) up -d --force-recreate --no-deps server
	node e2e/smoke/m06-context.mjs

e2e-smoke-m06-knowledge: compose-up
	MOCK_AGENT_RESPONSE=done $(COMPOSE) up -d --force-recreate --no-deps server
	$(COMPOSE) exec -T server sh -c 'if [ ! -d /tmp/smoke-repo/.git ]; then mkdir -p /tmp/smoke-repo; cd /tmp/smoke-repo; git init -b main; git config user.email smoke@coppice.local; git config user.name smoke; echo hi > README.md; git add .; git commit -m init; fi'
	node e2e/smoke/m06-knowledge.mjs

release-tar: web-build
	cargo build --release -p coppice-server -p coppice-cli
	mkdir -p dist/release/web
	cp target/release/coppice-server dist/release/
	cp target/release/coppice dist/release/coppice-cli
	cp -r web/dist dist/release/web/dist
	cp config.example.toml dist/release/config.example.toml
	cp -r deploy/systemd dist/release/systemd
	cp deploy/README-RELEASE.md dist/release/
	tar -czf dist/coppice-$$(uname -s | tr A-Z a-z)-$$(uname -m).tar.gz -C dist/release .
