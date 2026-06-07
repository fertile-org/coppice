COMPOSE = docker compose -f deploy/docker-compose.yml

.PHONY: compose-up compose-down test clippy migrate web-test web-dev

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

web-test:
	cd web && npm run test

web-dev:
	cd web && npm run dev
