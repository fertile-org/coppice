# Coppice release bundle

This directory is included in release tarballs produced by `make release-tar`.

## Layout

```
coppice-server          # API binary
coppice-cli             # CLI binary (invoke as `coppice` or rename/symlink)
web/dist/               # Built React SPA
config.example.toml     # Copy to config.toml
systemd/                # Example unit files
README-RELEASE.md       # This file
```

Rename `coppice-cli` to `coppice` on install if you prefer:

```bash
mv coppice-cli coppice
```

## Configure

Copy `config.example.toml` to `config.toml` in this directory (or use `~/.config/coppice/config.toml`).

Set `database.url` to your PostgreSQL instance.

## Run (recommended — API + web)

```bash
./coppice migrate
./coppice bootstrap admin --email admin@localhost --password changeme
./coppice server start    # terminal 1 — API on :5000
./coppice web start       # terminal 2 — UI on :5001, proxies /api to API
```

Open http://127.0.0.1:5001

## systemd

Copy and edit the example units:

```bash
sudo cp systemd/coppice-server.service /etc/systemd/system/
sudo cp systemd/coppice-web.service /etc/systemd/system/
# Edit WorkingDirectory and ExecStart paths
sudo systemctl daemon-reload
sudo systemctl enable --now coppice-server coppice-web
```

## Build a release tarball

From the repo root:

```bash
make release-tar
```
