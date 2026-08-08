# M08 — Connector operator CLI

## Goal

Give operators a first-class `coppice connector …` workflow to enable, diagnose, and (optionally) install real agent CLIs for Docker Compose and host installs — without baking vendor CLIs into the default server image or relying only on copy-paste compose overrides.

After this milestone, setting up `cursor` / `claude-code` / `codex` / `kilo-code` / `opencode` is a documented CLI path that works the same on the host and via `docker compose exec` on the **server** service.

## Product scope

### CLI surface (extend existing `cli/` binary)

Prefer extending **`coppice`** (already ships `migrate`, `bootstrap`, `health`, `server`, `web`). Do **not** introduce a separate `coppice-cli` binary.

```text
coppice connector list
coppice connector enable <id> [--config PATH]
coppice connector doctor <id>
coppice connector setup <id>          # interactive login wrapper (TTY)
coppice connector compose-snippet <id>
coppice connector install <id>        # optional / phase 2 — see below
```

| Command | Behavior |
|---------|----------|
| `list` | Known connectors + enabled? + binary on PATH? + auth hint |
| `enable` | Patch `[agent.connectors.<id>]` in the active config (`enabled = true`, default `model_providers` / `command` when missing) |
| `doctor` | Non-interactive checks: binary, version, auth files/env, `models` listing if the connector supports it; clear next-step errors |
| `setup` | Run the connector’s host login (`agent login`, `claude login`, …) with inherited TTY/env; print success/failure |
| `compose-snippet` | Print a ready-to-save `docker-compose.<id>.yml` override (same content class as [docs/providers/](../providers/README.md#docker-compose-host-clis)) |
| `install` | **Phase 2:** download/install CLI into a Coppice-managed directory on a persistent volume (see Architecture) |

### Operator UX principles

- Default Compose stack stays **`mock`-only**; no vendor CLIs in the default image.
- Same commands on host and in-container (`docker compose exec -it -u "$(id -u):$(id -g)" server coppice connector …`).
- Always target the **server** container (workers spawn CLIs there) — never the web service.
- Docs in [docs/providers/](../providers/) point at these commands; keep per-connector copy-paste overrides as fallback.

### Config + Docker

- `enable` writes the Docker config path when `COPPICE_CONFIG` / `deploy/config/config.toml` is in use.
- Document `HOME` / `PATH` expectations for in-container `setup` / `doctor` when operators use compose overrides or phase-2 volumes.
- Optional compose profile or override template that mounts a **connector data volume** once `install` exists.

## Out of scope

- Baking Cursor / Claude / Codex / Kilo / OpenCode into `deploy/Dockerfile.server` by default
- Auto-mounting host `~/.local` / `~/.config` in default `deploy/docker-compose.yml`
- GUI Settings UI for connector install (CLI-first; Settings can come later)
- Managing OAuth browser redirects inside a headless agent (document `docker exec -it` + host browser constraints)
- Changing connector runtime adapters (`server/src/providers/*`) beyond what doctor needs to invoke for checks
- CI using real connectors (continue `MockProvider`)

## Dependencies

- M01–M07: connectors and provider docs exist; M07 secrets/git remain independent
- Existing `cli/` crate and server image that can include the `coppice` binary (or a slim install of it) on PATH inside the server container

## Architecture notes

### Phased delivery

**Phase 1 (required for M08 acceptance)** — diagnose + enable + snippet + setup wrapper:

```text
cli/src/commands/connector/
  mod.rs
  list.rs
  enable.rs
  doctor.rs
  setup.rs
  compose_snippet.rs
```

- `doctor` / `setup` shell out to the configured `command` for each connector (from config defaults when disabled).
- `compose-snippet` embeds or generates YAML matching provider docs (single source of truth preferred: shared templates under `cli/templates/connectors/` or `docs/providers/snippets/` consumed by both docs and CLI).

**Phase 2 (same milestone if time; otherwise explicitly deferred in acceptance)** — managed install:

```text
/var/lib/coppice/connectors/<id>/   # versioned install root (volume)
PATH prepend or config command= absolute path into that root
```

- `install cursor` (etc.) downloads into the managed root; survives container recreate via a named volume.
- Still no default image bloat; operators opt in with a volume + `install`.

### In-container packaging

- Ship `coppice` on PATH in the **server** image (same binary as host CLI), or document installing it beside `coppice-server`.
- Entrypoint continues to drop to `COPPICE_UID`/`COPPICE_GID`; connector commands must run as that user so auth files stay owned correctly.

### Per-connector adapters (CLI only)

Small registry mirroring server connector IDs:

| ID | setup | doctor probes |
|----|-------|----------------|
| `cursor` | `agent login` | `agent models` / binary + `~/.config/cursor` |
| `claude-code` | `claude login` | `claude --version` (+ auth dir or `ANTHROPIC_API_KEY`) |
| `codex` | `codex login` | `codex --version` / models debug if available |
| `kilo-code` | document TUI `/connect` or `kilo auth login` | `kilo --version` / `kilo models …` |
| `opencode` | `opencode auth login` | `opencode auth list` / `opencode models …` |
| `mock` | n/a | always healthy |

## Testing strategy

### Unit / CLI tests

- `enable` patches TOML idempotently (enabled flag + default model_providers)
- `compose-snippet` output is valid YAML and includes expected volume keys per connector
- `doctor` returns non-zero when binary missing; zero when mocked PATH fixtures present

### Manual / smoke (not CI default)

```bash
# host
coppice connector doctor cursor

# compose (server)
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector doctor cursor
```

Document in [docs/providers/README.md](../providers/README.md) and [docs/development.md](../development.md).

## Acceptance criteria

- [ ] `coppice connector list|enable|doctor|setup|compose-snippet` implemented for all non-mock connectors above
- [ ] `coppice` binary available in the server container on PATH (or documented equivalent)
- [ ] `enable` updates `deploy/config/config.toml` / `COPPICE_CONFIG` correctly; server recreate picks up connector
- [ ] `doctor` fails clearly when CLI or auth is missing; succeeds after host or volume-mounted CLI works
- [ ] `compose-snippet` matches provider doc override shape; docs link to CLI as primary path
- [ ] Default `make compose-up` / CI smoke still use `mock` only — no vendor CLI required
- [ ] Exec target documented as **server**, not web
- [ ] Phase 2 `install` either shipped with a persistent volume recipe **or** explicitly listed under “Deferred to follow-up” in this doc’s changelog when M08 closes

## Deferred / follow-up

- Settings UI for connector health (surface `doctor` results)
- Non-interactive device-code login flows where vendors support them
- Auto-updating managed installs
- Windows/macOS path variants beyond Linux self-host

## Related docs

- [docs/providers/README.md](../providers/README.md) — Docker Compose (host CLIs)
- Per-connector Docker sections under [docs/providers/](../providers/)
- Existing CLI: `cli/` (`coppice migrate`, `bootstrap`, …)
