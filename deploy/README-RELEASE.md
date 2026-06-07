# Coppice release bundle

This directory is included in release tarballs produced by `make release-tar`.

## Layout

```
coppice-server          # API + SPA server binary
coppice-cli             # CLI binary
web/dist/               # Built React SPA
default.yaml            # Default config (copy and customize)
README-RELEASE.md       # This file
```

## Run the server

1. Copy `default.yaml` and set storage paths for your environment.
2. Point static serving at the bundled SPA:

```bash
export COPPICE_STORAGE__STATIC_DIR=./web/dist
./coppice-server
```

The server listens on `:8080` by default (`server.port` in config). Open `http://localhost:8080` in a browser for the UI; API routes remain under `/api` and `/health`.

3. Ensure PostgreSQL is reachable at the URL in config and run migrations:

```bash
./coppice-cli migrate
./coppice-cli bootstrap admin --email admin@localhost --password changeme
```

## Build a release tarball

From the repo root:

```bash
make release-tar
```

The tarball is written to `dist/coppice-<os>-<arch>.tar.gz`.
