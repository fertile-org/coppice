COMPOSE = docker compose -f deploy/docker-compose.yml

.PHONY: compose-up compose-down test clippy migrate web-test web-dev web-build release-tar

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

web-build:
	cd web && npm ci && npm run build

release-tar: web-build
	cargo build --release -p coppice-server -p coppice-cli
	mkdir -p dist/release
	cp target/release/coppice-server target/release/coppice-cli dist/release/
	cp -r web/dist dist/release/web/dist
	cp deploy/config/default.yaml dist/release/
	cp deploy/README-RELEASE.md dist/release/
	tar -czf dist/coppice-$$(uname -s | tr A-Z a-z)-$$(uname -m).tar.gz -C dist/release .
