# Prefer Docker Compose v2 plugin; fall back to docker-compose standalone (Homebrew).
ifeq ($(shell docker compose version >/dev/null 2>&1 && echo yes),yes)
DOCKER_COMPOSE = docker compose
else
DOCKER_COMPOSE = docker-compose
endif

COMPOSE = $(DOCKER_COMPOSE) -f deploy/docker-compose.yml
COMPOSE_LOCAL = $(DOCKER_COMPOSE) -f deploy/docker-compose.local.yml
LOCAL_DATABASE_URL = postgres://coppice:coppice@localhost:5433/coppice
LOCAL_SERVER_URL = http://localhost:8081
BOOTSTRAP_EMAIL = admin@localhost
BOOTSTRAP_PASSWORD = changeme

.PHONY: compose-up compose-down compose-local-up compose-local-down test clippy migrate migrate-local bootstrap bootstrap-local web-install web-test web-dev web-dev-local web-build e2e-smoke e2e-smoke-m03 release-tar

compose-up:
	$(COMPOSE) up -d --build

compose-down:
	$(COMPOSE) down

compose-local-up:
	$(COMPOSE_LOCAL) up -d --build

compose-local-down:
	$(COMPOSE_LOCAL) down

migrate:
	cargo run -p coppice-cli -- migrate

migrate-local:
	DATABASE_URL=$(LOCAL_DATABASE_URL) cargo run -p coppice-cli -- migrate

bootstrap:
	cargo run -p coppice-cli -- bootstrap admin --email $(BOOTSTRAP_EMAIL) --password $(BOOTSTRAP_PASSWORD)

bootstrap-local:
	COPPICE_SERVER_URL=$(LOCAL_SERVER_URL) cargo run -p coppice-cli -- bootstrap admin --email $(BOOTSTRAP_EMAIL) --password $(BOOTSTRAP_PASSWORD)

test:
	cargo test --workspace

clippy:
	cargo clippy --workspace -- -D warnings

web-install:
	cd web && yarn install --frozen-lockfile

web-test:
	cd web && yarn install --frozen-lockfile && yarn test

web-dev:
	cd web && yarn install && yarn dev

web-dev-local:
	cd web && yarn install && VITE_API_URL=http://localhost:8081 yarn dev

web-build:
	cd web && yarn install --frozen-lockfile && yarn build

e2e-smoke: compose-up
	node e2e/smoke/m02-board.mjs

e2e-smoke-m03: compose-up
	node e2e/smoke/m03-agent-run.mjs

release-tar: web-build
	cargo build --release -p coppice-server -p coppice-cli
	mkdir -p dist/release/web
	cp target/release/coppice-server dist/release/
	cp target/release/coppice dist/release/coppice-cli
	cp -r web/dist dist/release/web/dist
	cp deploy/config/default.yaml dist/release/
	cp deploy/README-RELEASE.md dist/release/
	tar -czf dist/coppice-$$(uname -s | tr A-Z a-z)-$$(uname -m).tar.gz -C dist/release .
