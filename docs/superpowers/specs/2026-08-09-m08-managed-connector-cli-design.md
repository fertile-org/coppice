# M08 Managed Connector Operator CLI Design

**Status:** Draft — awaiting user review  
**Date:** 2026-08-09  
**Topic:** Formalize M08: managed `$HOME` volume + `coppice connector` for real agent CLIs (no host bind-mount workarounds).  
**Milestone:** [M08-connector-operator-cli.md](../../milestones/M08-connector-operator-cli.md)

## Decision summary

Coppice operators enable, install, authenticate, and diagnose real agent connectors through the existing `coppice` CLI and a **single Docker path**: named volume `connector_data` mounted at `/home/coppice` as `HOME`. Vendor CLIs and auth live in that volume. The default server image stays **mock-only**; CI and smoke stay on mock.

This design **locks the contract already drafted in-tree** and defines the tighten pass: fix doctor/auth false positives, ensure PATH covers Cursor and OpenCode install layouts, automate `install` enough for **Cursor** and **OpenCode**, and prove both on Compose without host `~/.local` / `~/.config` mounts.

Host bind-mount override recipes and `compose-snippet` are **not** product surfaces.

## Goals

- One supported Compose path for real connectors: managed HOME volume + `coppice connector …` inside the **server** container.
- CLI: `list | enable | doctor | setup | install` on the existing `coppice` binary.
- Acceptance for this pass:
  1. **Cursor:** `install → setup → doctor` in Compose; models API works with no host mounts.
  2. **OpenCode:** install + setup into the same HOME until `doctor opencode` is green (binary + auth probe). Live `opencode serve` attach for a full agent run is not the gate.
- Other connectors (`claude-code`, `codex`, `kilo-code`): `enable` / `setup` / `doctor` + clear manual-into-HOME install hints — no full install automation in this pass.
- Docs point at the CLI + managed volume; strip host-mount workarounds.

## Non-goals

- Baking Cursor / Claude / Codex / Kilo / OpenCode into the default server image.
- Host bind-mount recipes as a supported path (including `compose-snippet`).
- Settings UI for connector health or install.
- Automated install for Claude / Codex / Kilo.
- CI jobs that exercise real vendor CLIs (continue `MockProvider`).
- Finishing remaining M07 sandbox/signals work.
- Making a full OpenCode agent run (serve + attach) the M08 gate.

## Docker contract

```text
Operator
  → docker compose exec -it -u $UID:$GID server coppice connector …
  → writes binaries + auth under /home/coppice (volume connector_data)
  → coppice-server workers spawn the same binaries with the same HOME/PATH
```

| Piece | Requirement |
|-------|-------------|
| Volume | `connector_data` → `/home/coppice` |
| `HOME` | `/home/coppice` (Compose + image default) |
| `PATH` | Prepend `/home/coppice/.local/bin` **and** `/home/coppice/.opencode/bin`; keep `/usr/sbin` (and `/sbin`) so entrypoint `gosu` works |
| Binary | Ship `coppice` next to `coppice-server` in `deploy/Dockerfile.server` |
| Entrypoint | After `gosu`, re-apply Compose `HOME`/`PATH`; `chown` `$HOME` for `COPPICE_UID`/`GID` |
| Config | `COPPICE_CONFIG=/etc/coppice/config.toml` bind-mounted **writable** so in-container `enable` can patch the host `deploy/config/config.toml` |
| Exec target | Always **server**, never web |

## CLI surface

| Command | Behavior |
|---------|----------|
| `list` | Known IDs + enabled? + binary on PATH? + short auth hint |
| `enable <id> [--config PATH]` | Patch `[agent.connectors.<id>]`: `enabled = true`; default `model_providers` / `command` when missing. Resolve path: `--config` → `COPPICE_CONFIG` → `./config.toml` if present → `deploy/config/config.toml` if present → else local path. Print restart/recreate reminder. |
| `doctor <id>` | Check binary on PATH; auth under `$HOME` (paths/env) with **conservative** heuristics; optional probe (`agent models`, `opencode auth list`, `--version`, …). Non-zero + next-step text on failure. |
| `setup <id>` | Vendor login with inherited TTY (see auth matrix). |
| `install <id>` | Cursor: upstream installer into `$HOME` (typically `$HOME/.local/bin/agent`). OpenCode: upstream installer into `$HOME` (typically `$HOME/.opencode/bin/opencode`). Others: print clear “not automated” + put binary on managed PATH. |

No `compose-snippet` command.

### Auth matrix (`setup`)

| ID | Behavior |
|----|----------|
| `cursor` | `agent login` — operator copies URL into a host browser if needed |
| `opencode` | `opencode auth login` |
| `codex` | `codex login --device-auth` |
| `claude-code` | Prefer `ANTHROPIC_API_KEY` or `claude setup-token` (paste); document that browser OAuth is unreliable in headless containers |
| `kilo-code` | Vendor auth CLI / TUI `/connect` / paste instructions |
| `mock` | n/a |

### Doctor auth heuristics (tighten)

Auth “ok” must not fire merely because a parent directory exists empty or because unrelated files are present. Prefer:

- Cursor: `$HOME/.config/cursor/auth.json` (or documented equivalent) exists and is non-empty, **or** models probe succeeds.
- OpenCode: evidence of completed auth (e.g. successful `opencode auth list`, or known auth store files under `$HOME` that indicate credentials — not merely `$HOME/.opencode` existing after install).
- Env-based connectors: non-empty env var (e.g. `ANTHROPIC_API_KEY`) counts as auth.

If binary is present but auth is missing, exit non-zero and point at `setup`.

## Components (draft → tighten)

Existing layout (keep; polish as needed):

```text
cli/src/commands/connector/
  mod.rs
  registry.rs    # id → binary, default providers, auth paths/env, hints
  list.rs
  enable.rs      # toml_edit; unit-tested
  doctor.rs
  setup.rs
  install.rs     # cursor + opencode install; others defer with messages
```

Deploy:

- `deploy/Dockerfile.server` — build/copy `coppice`; `curl` available for install scripts.
- `deploy/docker-compose.yml` — `connector_data`, `HOME`, expanded `PATH`, writable config mount.
- `deploy/entrypoint.sh` — preserve `HOME`/`PATH`; chown managed home.

Docs:

- Milestone M08, `docs/providers/*`, `docs/development.md`, `AGENTS.md` — managed connectors only; no host-mount override sections.

## Error handling

| Situation | Operator-facing behavior |
|-----------|--------------------------|
| Unknown connector id | Non-zero; list known IDs |
| Binary missing | Non-zero; suggest `install` or manual into `$HOME/.local/bin` / `.opencode/bin` |
| Auth missing | Non-zero; suggest `setup` + matrix hint |
| Probe fails after binary+auth look present | Non-zero; print truncated stderr/stdout |
| `enable` cannot write config | Non-zero; explain path and permissions / mount |
| After successful `enable` | Always print server recreate/restart reminder |

## Testing strategy

**Automated**

- `enable` patches TOML idempotently (cursor / claude-code style cases).
- `doctor` returns error when binary is absent (isolated `PATH`/`HOME` fixture).
- `cargo test -p coppice-cli`; `cargo clippy -p coppice-cli -- -D warnings`.

**Manual (acceptance gate)**

```bash
# Cursor
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector install cursor
# … setup cursor … doctor cursor
# Models API / Agents UI for cursor works without host CLI mounts

# OpenCode
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector install opencode
# … setup opencode … doctor opencode green
```

Default `make compose-up` / CI smoke remain mock-only.

## Implementation order (for writing-plans)

1. Spec + milestone alignment (this document; keep M08 milestone as operator summary).
2. Tighten doctor auth heuristics; expand Compose `PATH` for `.opencode/bin`.
3. Implement / harden `install opencode` (vendor script + `HOME`).
4. Docs/registry hints consistency for OpenCode PATH and auth.
5. Verify Cursor + OpenCode on Compose; clippy/tests; commit when requested.

## Open questions

None blocking. OpenCode “doctor green” is explicitly the second-connector gate, not a full serve/attach run.
