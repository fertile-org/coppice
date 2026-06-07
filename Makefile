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
