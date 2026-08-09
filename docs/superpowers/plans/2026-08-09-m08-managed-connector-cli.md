# M08 Managed Connector CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish M08 by locking the managed-`$HOME` Docker contract and `coppice connector` CLI so Cursor (install→setup→doctor + models API) and OpenCode (install→setup→doctor green) work on Compose without host CLI bind-mounts.

**Architecture:** Draft already exists in the working tree (`cli/src/commands/connector/*`, Compose `connector_data`, server image ships `coppice`). This plan commits that baseline, then tightens doctor auth heuristics, expands `PATH` for `$HOME/.opencode/bin`, automates `install opencode`, aligns docs, and verifies Cursor + OpenCode on Compose.

**Tech Stack:** Rust (`coppice-cli`, clap, toml_edit, which), Docker Compose, existing `coppice-server` connector adapters

**Spec:** [docs/superpowers/specs/2026-08-09-m08-managed-connector-cli-design.md](../specs/2026-08-09-m08-managed-connector-cli-design.md)

---

## File map

| File | Responsibility |
|------|----------------|
| `cli/src/commands/connector/registry.rs` | Connector IDs, binaries, auth signals, `auth_present` |
| `cli/src/commands/connector/enable.rs` | TOML patch for `[agent.connectors.<id>]` |
| `cli/src/commands/connector/doctor.rs` | Binary / auth / probe checks |
| `cli/src/commands/connector/setup.rs` | Vendor login wrappers |
| `cli/src/commands/connector/install.rs` | Cursor + OpenCode installers; others deferred |
| `cli/src/commands/connector/list.rs` | Status table |
| `cli/src/commands/connector/mod.rs` | Subcommand wiring |
| `cli/src/main.rs`, `cli/src/commands/mod.rs`, `cli/Cargo.toml` | Wire `connector` + deps |
| `deploy/Dockerfile.server` | Build/copy `coppice`; `curl`; default `HOME` |
| `deploy/docker-compose.yml` | `connector_data`, `HOME`, `PATH` (incl. `.opencode/bin`) |
| `deploy/entrypoint.sh` | Preserve `HOME`/`PATH` after `gosu`; chown `$HOME` |
| `docs/providers/*`, `docs/development.md`, `docs/milestones/M08-*.md`, `AGENTS.md` | Managed-HOME docs only |

---

### Task 1: Baseline — verify draft CLI + deploy, commit

**Files (must already exist from draft; create only if missing):**
- `cli/src/commands/connector/{mod,registry,list,enable,doctor,setup,install}.rs`
- `cli/src/main.rs` — `Commands::Connector`
- `deploy/Dockerfile.server`, `deploy/docker-compose.yml`, `deploy/entrypoint.sh`
- Docs stripped of host-mount overrides (providers + development)

- [ ] **Step 1: Confirm baseline commands compile and unit tests pass**

Run:

```bash
cargo test -p coppice-cli
cargo clippy -p coppice-cli -- -D warnings
```

Expected: all tests pass (at least `enable_*` and `doctor_fails_when_binary_missing`); clippy clean.

If `connector` module is missing, recreate from the design + this plan’s later tasks before continuing (do not invent `compose-snippet`).

- [ ] **Step 2: Confirm Compose contract keys exist**

In `deploy/docker-compose.yml` under `server.environment` / `volumes` / top-level `volumes`, confirm:

- `HOME: /home/coppice`
- `PATH` includes `/home/coppice/.local/bin` and `/usr/sbin`
- `connector_data:/home/coppice`
- Config mount **without** `:ro` (writable for `enable`)
- Named volume `connector_data:`

In `deploy/Dockerfile.server`, confirm `coppice` binary is copied and `curl` is installed.

- [ ] **Step 3: Commit baseline**

```bash
git add \
  AGENTS.md Cargo.lock cli/ \
  deploy/Dockerfile.server deploy/docker-compose.yml deploy/entrypoint.sh \
  docs/development.md docs/milestones/M08-connector-operator-cli.md docs/milestones/README.md \
  docs/providers/
git status
git commit -m "$(cat <<'EOF'
feat(cli): add managed-HOME connector operator commands

Ship coppice connector list|enable|doctor|setup|install, connector_data
volume, and docs that point at the managed HOME path instead of host mounts.
EOF
)"
```

---

### Task 2: Conservative `auth_present` (TDD)

**Files:**
- Modify: `cli/src/commands/connector/registry.rs`
- Test: same file (`#[cfg(test)]`)

- [ ] **Step 1: Write failing tests for auth heuristics**

Append to `registry.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn auth_rejects_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let auth = dir.path().join(".config/cursor/auth.json");
        fs::create_dir_all(auth.parent().unwrap()).unwrap();
        fs::File::create(&auth).unwrap(); // empty
        let m = meta(ConnectorId::Cursor);
        assert!(!auth_present(m, dir.path()));
    }

    #[test]
    fn auth_accepts_non_empty_auth_json() {
        let dir = tempfile::tempdir().unwrap();
        let auth = dir.path().join(".config/cursor/auth.json");
        fs::create_dir_all(auth.parent().unwrap()).unwrap();
        let mut f = fs::File::create(&auth).unwrap();
        writeln!(f, "{{ \"token\": \"x\" }}").unwrap();
        let m = meta(ConnectorId::Cursor);
        assert!(auth_present(m, dir.path()));
    }

    #[test]
    fn auth_rejects_empty_opencode_install_dir() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".opencode/bin")).unwrap();
        let m = meta(ConnectorId::OpenCode);
        assert!(
            !auth_present(m, dir.path()),
            "install tree alone must not count as auth"
        );
    }

    #[test]
    fn auth_accepts_anthropic_api_key_env() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let m = meta(ConnectorId::ClaudeCode);
        assert!(auth_present(m, dir.path()));
        std::env::remove_var("ANTHROPIC_API_KEY");
    }
}
```

- [ ] **Step 2: Run tests — expect failures**

```bash
cargo test -p coppice-cli auth_rejects_empty_file auth_rejects_empty_opencode_install_dir -- --exact
```

Expected: FAIL (empty file / `.opencode` currently count as auth, or OpenCode still lists `.opencode` in `auth_paths`).

- [ ] **Step 3: Implement conservative `auth_present` + fix OpenCode paths**

In `registry.rs`:

1. Change OpenCode `auth_paths` to auth-store only (not install root):

```rust
    ConnectorMeta {
        id: ConnectorId::OpenCode,
        binary: "opencode",
        default_model_providers: &[],
        auth_hint: "opencode auth login",
        // After login; do NOT list `.opencode` (install tree).
        auth_paths: &[".local/share/opencode"],
        auth_env: &[],
    },
```

2. Replace `auth_present` with:

```rust
pub fn auth_present(meta: &ConnectorMeta, home: &Path) -> bool {
    for key in meta.auth_env {
        if std::env::var_os(key).is_some_and(|v| !v.is_empty()) {
            return true;
        }
    }
    for rel in meta.auth_paths {
        let p = home.join(rel);
        if path_looks_like_auth(&p) {
            return true;
        }
    }
    false
}

fn path_looks_like_auth(path: &Path) -> bool {
    if path.is_file() {
        return std::fs::metadata(path)
            .map(|m| m.len() > 0)
            .unwrap_or(false);
    }
    if path.is_dir() {
        return std::fs::read_dir(path)
            .ok()
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false);
    }
    false
}
```

Also tighten Claude/Codex/Kilo: prefer specific files where known; directories must be **non-empty** (handled by `path_looks_like_auth`). Leave Claude `auth_paths` as `.claude` / `.config/claude` but non-empty-dir rule applies.

- [ ] **Step 4: Run tests — expect pass**

```bash
cargo test -p coppice-cli -- auth_
```

Expected: PASS for the new auth tests.

- [ ] **Step 5: Commit**

```bash
git add cli/src/commands/connector/registry.rs
git commit -m "$(cat <<'EOF'
fix(cli): require non-empty auth signals for connector doctor

Avoid treating empty files or OpenCode install trees as authenticated.
EOF
)"
```

---

### Task 3: Doctor — probe as auth evidence + clearer next steps

**Files:**
- Modify: `cli/src/commands/connector/doctor.rs`

- [ ] **Step 1: Write failing test — auth missing when only empty home**

```rust
#[test]
fn doctor_fails_when_auth_missing_even_if_binary_exists() {
    let dir = tempfile::tempdir().unwrap();
    // Fake `agent` on PATH
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let agent = bin.join("agent");
    std::fs::write(&agent, "#!/bin/sh\necho no\nexit 1\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&agent).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&agent, perms).unwrap();
    }
    std::env::set_var("PATH", &bin);
    std::env::set_var("HOME", dir.path());
    let err = run(DoctorArgs {
        id: "cursor".into(),
    });
    assert!(err.is_err());
}
```

- [ ] **Step 2: Run test**

```bash
cargo test -p coppice-cli doctor_fails_when_auth_missing_even_if_binary_exists -- --exact
```

Expected: PASS already if auth heuristics work; if it unexpectedly PASSes doctor, adjust until auth missing fails. (If probe is skipped when auth missing, this should already fail — keep that behavior.)

- [ ] **Step 3: Update doctor flow for probe-as-auth fallback**

Replace the auth + probe section in `run` so Cursor/OpenCode can treat a successful probe as auth when files are absent (matches spec), without skipping probe when files look present:

```rust
    let mut auth_ok = auth_present(m, &home);
    let binary_ok = binary_on_path(m.binary).is_some();

    // ... keep binary println / failed flag as today ...

    if binary_ok {
        match probe_models(id, m.binary) {
            Ok(()) => {
                println!("models probe: ok");
                if !auth_ok {
                    println!("auth: ok (via models/auth probe)");
                    auth_ok = true;
                } else {
                    println!("auth: ok ({})", m.auth_hint);
                }
            }
            Err(e) => {
                if auth_ok {
                    println!("auth: ok ({})", m.auth_hint);
                    println!("models probe: FAIL ({e})");
                    failed = true;
                } else {
                    println!("auth: MISSING");
                    println!("  next: coppice connector setup {id}");
                    println!("  hint: {}", m.auth_hint);
                    println!("models probe: FAIL ({e})");
                    failed = true;
                }
            }
        }
    } else if auth_ok {
        println!("auth: ok ({})", m.auth_hint);
    } else {
        println!("auth: MISSING");
        println!("  next: coppice connector setup {id}");
        println!("  hint: {}", m.auth_hint);
        failed = true;
    }
```

Remove the old “only probe when `!failed`” block so it does not double-print.

For OpenCode missing-binary message, mention `.opencode/bin`:

```rust
            println!(
                "  next: coppice connector install {id}  (ensure PATH includes $HOME/.local/bin and $HOME/.opencode/bin)"
            );
```

- [ ] **Step 4: Run all CLI tests + clippy**

```bash
cargo test -p coppice-cli
cargo clippy -p coppice-cli -- -D warnings
```

Expected: PASS / clean.

- [ ] **Step 5: Commit**

```bash
git add cli/src/commands/connector/doctor.rs
git commit -m "$(cat <<'EOF'
fix(cli): use connector probes as auth evidence in doctor

Align doctor with the M08 auth matrix and clearer PATH next steps.
EOF
)"
```

---

### Task 4: Compose PATH + `install opencode`

**Files:**
- Modify: `deploy/docker-compose.yml`
- Modify: `cli/src/commands/connector/install.rs`

- [ ] **Step 1: Expand Compose PATH**

In `deploy/docker-compose.yml` `server.environment`, set:

```yaml
      HOME: /home/coppice
      PATH: /home/coppice/.local/bin:/home/coppice/.opencode/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
```

- [ ] **Step 2: Implement `install_opencode`**

In `install.rs`, replace the OpenCode `defer_or_hint` arm with `install_opencode(&home)?`.

Add:

```rust
fn install_opencode(home: &std::path::Path) -> anyhow::Result<()> {
    let opencode_bin = home.join(".opencode/bin");
    std::fs::create_dir_all(&opencode_bin)?;

    let path = std::env::var("PATH").unwrap_or_default();
    let prepended = format!("{}:{}:{path}", home.join(".local/bin").display(), opencode_bin.display());
    std::env::set_var("PATH", &prepended);

    if binary_on_path("opencode").is_some() {
        println!("`opencode` already on PATH; skipping download");
        return Ok(());
    }

    println!("Installing OpenCode into HOME={} …", home.display());
    let status = Command::new("sh")
        .arg("-c")
        .arg("curl -fsSL https://opencode.ai/install | bash")
        .env("HOME", home)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| anyhow::anyhow!("failed to run opencode install (need curl?): {e}"))?;
    if !status.success() {
        anyhow::bail!("opencode install script exited with {status}");
    }
    Ok(())
}
```

At the start of `run`, also prepend `.opencode/bin` (not only `.local/bin`):

```rust
    let local_bin = home.join(".local/bin");
    let opencode_bin = home.join(".opencode/bin");
    std::fs::create_dir_all(&local_bin)?;
    std::fs::create_dir_all(&opencode_bin)?;
    let path = std::env::var("PATH").unwrap_or_default();
    let prepended = format!("{}:{}:{path}", local_bin.display(), opencode_bin.display());
    std::env::set_var("PATH", &prepended);
    std::env::set_var("HOME", &home);
```

Update the “still not on PATH” message to print both dirs.

- [ ] **Step 3: Unit-level smoke (no network)**

```bash
cargo test -p coppice-cli
cargo clippy -p coppice-cli -- -D warnings
```

Expected: PASS. (Do not run real curl install in unit tests.)

- [ ] **Step 4: Commit**

```bash
git add deploy/docker-compose.yml cli/src/commands/connector/install.rs
git commit -m "$(cat <<'EOF'
feat(cli): install OpenCode into managed HOME

Prepend .opencode/bin on Compose PATH and automate connector install opencode.
EOF
)"
```

---

### Task 5: Docs alignment (OpenCode PATH + acceptance)

**Files:**
- Modify: `docs/providers/README.md`
- Modify: `docs/providers/opencode.md`
- Modify: `docs/milestones/M08-connector-operator-cli.md`
- Modify: `docs/development.md` (only if PATH mention is incomplete)

- [ ] **Step 1: Providers README — document both bin dirs**

Under Docker Compose (managed connectors), ensure PATH note exists:

```markdown
Compose sets `HOME=/home/coppice` and prepends `/home/coppice/.local/bin` and `/home/coppice/.opencode/bin` to `PATH` (keeps `/usr/sbin` for `gosu`).
```

Include OpenCode in the example command block alongside cursor:

```bash
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector install opencode
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector setup opencode
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector doctor opencode
```

- [ ] **Step 2: `opencode.md` Setup section**

Ensure Setup shows:

```bash
coppice connector enable opencode
coppice connector install opencode
coppice connector setup opencode
coppice connector doctor opencode
```

Note: binary lands in `$HOME/.opencode/bin`; full serve/attach for a live run is separate from doctor green.

- [ ] **Step 3: Milestone acceptance**

In `docs/milestones/M08-connector-operator-cli.md`, ensure acceptance mentions Cursor **and** OpenCode doctor green; PATH includes `.opencode/bin`. Leave checkboxes unchecked until Task 6 verifies, or check only items already true after Tasks 1–5.

- [ ] **Step 4: Commit**

```bash
git add docs/providers/README.md docs/providers/opencode.md docs/milestones/M08-connector-operator-cli.md docs/development.md
git commit -m "$(cat <<'EOF'
docs: align M08 provider docs with OpenCode managed PATH

Document .opencode/bin and the install/setup/doctor loop for OpenCode.
EOF
)"
```

---

### Task 6: Manual Compose verification (acceptance gate)

**Files:** none (operator verification). Rebuild server image so `coppice` and PATH changes land.

- [ ] **Step 1: Rebuild and recreate server**

```bash
make compose-up
# or:
docker compose -f deploy/docker-compose.yml build server
docker compose -f deploy/docker-compose.yml up -d --force-recreate server
```

Confirm:

```bash
docker compose -f deploy/docker-compose.yml exec -u "$(id -u):$(id -g)" server \
  sh -c 'echo HOME=$HOME; echo PATH=$PATH; command -v coppice'
```

Expected: `HOME=/home/coppice`, PATH contains `.local/bin` and `.opencode/bin`, `coppice` on PATH.

- [ ] **Step 2: Cursor path**

```bash
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector enable cursor
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector install cursor
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector setup cursor
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector doctor cursor
```

Expected: `doctor: ok`. Then confirm models API / Agents UI for cursor works **without** any host `~/.local` / `~/.config` bind mounts on the server service.

- [ ] **Step 3: OpenCode path**

```bash
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector enable opencode
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector install opencode
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector setup opencode
docker compose -f deploy/docker-compose.yml exec -it -u "$(id -u):$(id -g)" server \
  coppice connector doctor opencode
```

Expected: `doctor: ok` (binary + auth list). Full `opencode serve` agent run is **not** required.

- [ ] **Step 4: Mark milestone acceptance checkboxes** that are now true; commit docs if needed

```bash
git add docs/milestones/M08-connector-operator-cli.md
git commit -m "$(cat <<'EOF'
docs(milestones): mark M08 acceptance after Compose verification
EOF
)"
```

(Skip commit if no checkbox edits.)

---

### Task 7: Final gate

- [ ] **Step 1: Final automated checks**

```bash
cargo test -p coppice-cli
cargo clippy -p coppice-cli -- -D warnings
```

Expected: PASS / clean.

- [ ] **Step 2: Optional workspace clippy if you touched only cli/deploy/docs**

```bash
cargo clippy -p coppice-cli -- -D warnings
```

(Full `make test` is **not** required for this pass per AGENTS.md fast-verification guidance.)

- [ ] **Step 3: Confirm no host-mount docs remain**

```bash
rg -n "host CLIs|compose-snippet|docker-compose\\.cursor" docs/providers docs/development.md docs/milestones/M08-connector-operator-cli.md || true
```

Expected: no supported-path recipes (mentions of removal / “do not bind-mount” are fine).

---

## Spec coverage check

| Spec requirement | Task |
|------------------|------|
| Managed HOME volume + no host mounts | 1, 4, 5, 6 |
| Ship `coppice` in server image | 1 |
| Preserve HOME/PATH after gosu | 1 (baseline entrypoint) |
| CLI list/enable/doctor/setup/install | 1 |
| No compose-snippet | 1 (never add) |
| Conservative doctor auth | 2, 3 |
| PATH includes `.opencode/bin` | 4 |
| `install cursor` + `install opencode` | 1 (cursor), 4 (opencode) |
| Cursor models API without host mounts | 6 |
| OpenCode doctor green | 6 |
| Docs point at managed path | 5 |
| Unit tests + clippy | 1–3, 7 |
| Others deferred install | 1 / 4 (unchanged defer arms) |

## Placeholder / consistency review

- No TBD steps; OpenCode install URL is `https://opencode.ai/install`.
- Auth helper names: `auth_present`, `path_looks_like_auth` used consistently in Tasks 2–3.
- Compose PATH string in Task 4 matches providers README in Task 5.
