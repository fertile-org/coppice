#!/usr/bin/env bash
# Exercise the installed Linux package's real coppice-server.
#
# The binary must be the one dpkg installed at
# /opt/Coppice/resources/bin/coppice-server. It is started with env -i, a short
# PATH, and a zsh login shell whose .zshrc puts a fake `agent` on PATH. That
# agent prints a captured `agent models` list that includes Auto.
#
# A fresh profile's config.toml is the generated desktop template: the locked
# header, layout paths, and no connector tables. Cursor still lists Auto.
# A second models?refresh=true calls `agent` again. A later start imports a
# stale ~/.config/coppice/config.toml by allowlist only; the desktop master key
# still decrypts a forge token, and Cursor still lists Auto.
set -euo pipefail

expected_commit=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --expected-commit)
      expected_commit="${2:?--expected-commit needs a sha}"
      shift 2
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [ -z "$expected_commit" ]; then
  expected_commit="$(git rev-parse HEAD)"
fi

if ! command -v dpkg >/dev/null; then
  echo "dpkg is required" >&2
  exit 1
fi
if ! command -v zsh >/dev/null; then
  echo "zsh is required" >&2
  exit 1
fi
if ! command -v node >/dev/null; then
  echo "node is required" >&2
  exit 1
fi

mapfile -t servers < <(dpkg -L coppice-desktop | grep -x '/opt/Coppice/resources/bin/coppice-server' || true)
if [ "${#servers[@]}" -ne 1 ] || [ ! -x "${servers[0]}" ]; then
  echo "dpkg -L coppice-desktop did not list the real coppice-server" >&2
  dpkg -L coppice-desktop | grep coppice-server || true
  exit 1
fi
SERVER="${servers[0]}"
RESOURCES="$(dirname "$(dirname "$SERVER")")"
ZSH="$(command -v zsh)"

ROOT="$(mktemp -d)"
HOME_DIR="$ROOT/home"
DATA="$HOME_DIR/.config/Coppice"
JAR="$ROOT/cookies"
LOG="$ROOT/server.log"
PID_FILE="$ROOT/server.pid"
CALLS="$HOME_DIR/agent-calls.log"
cleanup() {
  if [ -f "$PID_FILE" ]; then
    pid="$(cat "$PID_FILE" || true)"
    if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null; then
      kill -TERM "$pid" 2>/dev/null || true
      for _ in $(seq 1 20); do
        kill -0 "$pid" 2>/dev/null || break
        sleep 1
      done
      kill -KILL "$pid" 2>/dev/null || true
    fi
  fi
  rm -rf "$ROOT"
}
trap cleanup EXIT

mkdir -p "$HOME_DIR/bin" "$HOME_DIR/.config/coppice"
cat > "$HOME_DIR/.zshrc" <<EOF
export PATH="$HOME_DIR/bin:\$PATH"
EOF
cat > "$HOME_DIR/bin/agent" <<EOF
#!/bin/sh
printf '%s\n' "\$*" >> "$CALLS"
if [ -f "$HOME_DIR/.coppice-agent-empty" ]; then
  exit 0
fi
case "\$1" in
  models|--list-models)
    cat <<'MODELS'
Available models

auto - Auto (current, default)
gpt-5.3-codex-low - Codex 5.3 Low
composer-2.5 - Composer 2.5
claude-opus-5-thinking-high - Claude Opus 5 1M Thinking
claude-fable-5-thinking-high - Claude Fable 5 1M Thinking (NO ZDR)
grok-4.7-low - Grok 4.7  Low
cursor-grok-4.6-low-fast - Grok 4.6 Low Fast
MODELS
    ;;
esac
exit 0
EOF
chmod 755 "$HOME_DIR/bin/agent"

version_out="$(env -i PATH=/usr/bin:/bin HOME="$HOME_DIR" "$SERVER" --version)"
printf '%s\n' "$version_out"
commit_line="$(printf '%s\n' "$version_out" | sed -n '2p')"
if [ "$commit_line" != "commit $expected_commit" ]; then
  echo "expected commit $expected_commit, got: $commit_line" >&2
  exit 1
fi
case "$(printf '%s\n' "$version_out" | sed -n '1p')" in
  coppice-server\ *) ;;
  *)
    echo "version line is not coppice-server <version>" >&2
    exit 1
    ;;
esac

start_server() {
  : >"$LOG"
  env -i \
    PATH=/usr/bin:/bin \
    HOME="$HOME_DIR" \
    SHELL="$ZSH" \
    USER="${USER:-coppice}" \
    LOGNAME="${USER:-coppice}" \
    TMPDIR="${TMPDIR:-/tmp}" \
    LANG="${LANG:-C.UTF-8}" \
    "$SERVER" desktop --data-dir "$DATA" --resources "$RESOURCES" \
    >"$LOG" 2>&1 &
  echo $! >"$PID_FILE"
  local pid
  pid="$(cat "$PID_FILE")"
  local url=""
  for _ in $(seq 1 120); do
    if grep -q 'COPPICE_READY url=' "$LOG"; then
      url="$(grep -o 'http://127.0.0.1:[0-9]*' "$LOG" | tail -1)"
      break
    fi
    if ! kill -0 "$pid" 2>/dev/null; then
      echo "coppice-server exited before ready" >&2
      cat "$LOG" >&2
      exit 1
    fi
    sleep 1
  done
  if [ -z "$url" ]; then
    echo "timed out waiting for COPPICE_READY" >&2
    cat "$LOG" >&2
    exit 1
  fi
  BASE="$url"
}

stop_server() {
  local pid
  pid="$(cat "$PID_FILE")"
  kill -TERM "$pid" 2>/dev/null || true
  for _ in $(seq 1 20); do
    kill -0 "$pid" 2>/dev/null || break
    sleep 1
  done
  if kill -0 "$pid" 2>/dev/null; then
    echo "coppice-server did not exit" >&2
    exit 1
  fi
  rm -f "$PID_FILE"
}

session() {
  local body
  body="$(curl -sS -c "$JAR" -b "$JAR" -X POST "$BASE/api/auth/desktop-session")"
  CSRF="$(printf '%s' "$body" | node -e 'let s=""; process.stdin.on("data",d=>s+=d); process.stdin.on("end",()=>{ const j=JSON.parse(s); if(!j.csrfToken) process.exit(2); process.stdout.write(j.csrfToken); })')"
}

api() {
  local method="$1" path="$2"
  shift 2
  curl -sS -c "$JAR" -b "$JAR" -H "X-CSRF-Token: $CSRF" -H 'Content-Type: application/json' \
    -X "$method" "$BASE$path" "$@"
}

wait_cursor() {
  local body=""
  for _ in $(seq 1 40); do
    body="$(api GET /api/connectors)"
    if printf '%s' "$body" | node -e 'let s=""; process.stdin.on("data",d=>s+=d); process.stdin.on("end",()=>{ const j=JSON.parse(s); const c=(j.items||[]).find(i=>i.id==="cursor"); if(c && c.readiness==="ready" && c.enabled===true) process.exit(0); process.exit(1); })'; then
      return 0
    fi
    sleep 1
  done
  echo "Cursor was not ready in the registry" >&2
  printf '%s\n' "$body" >&2
  echo "--- agent calls ---" >&2
  cat "$CALLS" >&2 || true
  echo "--- server log ---" >&2
  cat "$LOG" >&2
  exit 1
}

assert_models_include_auto() {
  local body="$1"
  printf '%s' "$body" | node -e 'let s=""; process.stdin.on("data",d=>s+=d); process.stdin.on("end",()=>{ const j=JSON.parse(s); const hit=(j.items||[]).find(m=>m.id==="auto" && m.name==="Auto"); if(!hit) { console.error(s); process.exit(1);} })'
}

call_count() {
  if [ -f "$CALLS" ]; then
    grep -c . "$CALLS" || true
  else
    echo 0
  fi
}

echo "fresh profile"
start_server
session
wait_cursor

expected="$ROOT/expected-config.toml"
cat >"$expected" <<EOF
# Coppice desktop configuration. Edits here survive upgrades.
# Secrets (session secret, encryption key, passwords) live in secrets/, not here.
# The server port, database URL and auth mode are set by the desktop app at startup.
# Turn connectors on or off in Connectors in the app.

# How agents run.
[agent]
# Coppice gives each ticket its own git worktree in this folder.
worktrees_path = "${DATA}/worktrees"

# Logs and other files from agent runs and chats.
[storage]
artifacts_dir = "${DATA}/artifacts"

# Your default plugin folder. Coppice always looks here for plugins.
[plugins]
# Add more folders on the Plugins page. This one can't be removed there.
dir = "${DATA}/plugins"

# Skills that ship with Coppice.
[mcp]
# Coppice rewrites coppice/skills here on every start. Edits there are lost.
builtin_plugins_dir = "${DATA}/builtin-plugins"
EOF
if ! diff -u "$expected" "$DATA/config.toml"; then
  echo "fresh config.toml is not the desktop template" >&2
  exit 1
fi
if grep -q 'default_connector\|\[agent\.connectors' "$DATA/config.toml"; then
  echo "fresh template contains connector settings" >&2
  exit 1
fi

providers="$(api GET /api/connectors/cursor/model-providers)"
printf '%s' "$providers" | node -e 'let s=""; process.stdin.on("data",d=>s+=d); process.stdin.on("end",()=>{ const j=JSON.parse(s); if(!(j.items||[]).some(i=>i.id==="cursor")) { console.error(s); process.exit(1);} })'

before="$(call_count)"
models="$(api GET '/api/connectors/cursor/model-providers/cursor/models?refresh=true')"
assert_models_include_auto "$models"
after="$(call_count)"
if [ "$after" -le "$before" ]; then
  echo "refresh=true did not call agent (before=$before after=$after)" >&2
  exit 1
fi
models2="$(api GET '/api/connectors/cursor/model-providers/cursor/models?refresh=true')"
assert_models_include_auto "$models2"
after2="$(call_count)"
if [ "$after2" -le "$after" ]; then
  echo "second refresh did not call agent again (after=$after after2=$after2)" >&2
  exit 1
fi

unknown_code="$(api GET /api/connectors/cursor/model-providers/not-a-provider/models -o "$ROOT/unknown.json" -w '%{http_code}')"
if [ "$unknown_code" != "404" ]; then
  echo "unknown provider status $unknown_code" >&2
  cat "$ROOT/unknown.json" >&2
  exit 1
fi
grep -F "Unknown model provider 'not-a-provider'." "$ROOT/unknown.json" >/dev/null

touch "$HOME_DIR/.coppice-agent-empty"
empty_code="$(api GET '/api/connectors/cursor/model-providers/cursor/models?refresh=true' -o "$ROOT/empty.json" -w '%{http_code}')"
rm -f "$HOME_DIR/.coppice-agent-empty"
if [ "$empty_code" != "502" ]; then
  echo "empty model list status $empty_code" >&2
  cat "$ROOT/empty.json" >&2
  exit 1
fi
if ! grep -E 'returned no models|listed no models' "$ROOT/empty.json" >/dev/null; then
  echo "empty model list was not reported as a failure" >&2
  cat "$ROOT/empty.json" >&2
  exit 1
fi

missing_code="$(curl -sS -c "$JAR" -b "$JAR" -o /dev/null -w '%{http_code}' "$BASE/api/this-route-is-missing")"
if [ "$missing_code" != "404" ]; then
  echo "missing route status $missing_code" >&2
  exit 1
fi

repo="$ROOT/repo"
git init -q "$repo"
created="$(api POST /api/repos --data "{\"name\":\"e2e\",\"localPath\":\"$repo\",\"defaultBranch\":\"main\"}")"
repo_id="$(printf '%s' "$created" | node -e 'let s=""; process.stdin.on("data",d=>s+=d); process.stdin.on("end",()=>{ const j=JSON.parse(s); if(!j.id){console.error(s); process.exit(1);} process.stdout.write(j.id); })')"
api POST "/api/repos/$repo_id/forge-token" --data '{"token":"coppice-token-still-decrypts"}' >/dev/null

decrypt_token() {
  local port hex_ct hex_nonce
  port="$(sed -n '4p' "$DATA/pg/data/postmaster.pid")"
  read -r hex_ct hex_nonce < <(
    PGPASSWORD="$(cat "$DATA/secrets/pg_password")" \
      LD_LIBRARY_PATH="$RESOURCES/postgres/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
      "$RESOURCES/postgres/bin/psql" -h 127.0.0.1 -p "$port" -U coppice -d coppice -At -c \
      "SELECT encode(ciphertext,'hex') || ' ' || encode(nonce,'hex') FROM secrets WHERE name LIKE 'repo-forge-token-%'" \
      2>"$ROOT/psql.err"
  ) || {
    echo "psql failed" >&2
    cat "$ROOT/psql.err" >&2
    return 1
  }
  node - "$DATA/secrets/master_key" "$hex_ct" "$hex_nonce" <<'JS'
const { createDecipheriv, createHash } = require('crypto');
const fs = require('fs');
const master = fs.readFileSync(process.argv[2], 'utf8').trim();
const ct = Buffer.from(process.argv[3], 'hex');
const nonce = Buffer.from(process.argv[4], 'hex');
const tag = ct.subarray(ct.length - 16);
const data = ct.subarray(0, ct.length - 16);
const decipher = createDecipheriv('aes-256-gcm', createHash('sha256').update(master).digest(), nonce);
decipher.setAuthTag(tag);
process.stdout.write(Buffer.concat([decipher.update(data), decipher.final()]).toString());
JS
}

plain="$(decrypt_token)"
if [ "$plain" != "coppice-token-still-decrypts" ]; then
  echo "forge token did not decrypt before import" >&2
  exit 1
fi
master_before="$(cat "$DATA/secrets/master_key")"
stop_server

if ! grep -q 'duration_ms' "$LOG"; then
  echo "request log missing duration_ms" >&2
  exit 1
fi
if ! grep -q 'path=/api/this-route-is-missing' "$LOG" || ! grep -q 'status=404' "$LOG"; then
  echo "404 was not logged" >&2
  exit 1
fi
if ! grep -q 'listed models' "$LOG"; then
  echo "model list was not logged" >&2
  exit 1
fi
cp "$LOG" "$ROOT/first.log"

echo "legacy import"
cat > "$HOME_DIR/.config/coppice/config.toml" <<'EOF'
[database]
url = "postgres://evil/db"

[auth]
session_secret = "stolen-session"

[secrets]
master_key = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

[server]
port = 1

[storage]
artifacts_dir = "/tmp/evil-artifacts"

[web]
port = 9

[git]
author_name = "Imported Ada"
author_email = "ada@import.example"
push_enabled = true

[agent]
worker_count = 7
default_connector = "mock"
worktrees_path = "/evil/worktrees"

[agent.connectors.cursor]
enabled = false
command = "agent"
run_timeout_secs = 42

[knowledge.retrieval]
top_k = 4
EOF

start_server
session
notice="$(api GET /api/import-notice)"
printf '%s' "$notice" | node -e 'let s=""; process.stdin.on("data",d=>s+=d); process.stdin.on("end",()=>{ const j=JSON.parse(s); const want="Imported your git author, worker, knowledge and connector settings from `~/.config/coppice/config.toml`. Connectors stay as set in Connectors. The old file is now `config.toml.imported`."; if(j.message!==want){ console.error(j.message); process.exit(1);} })'
if [ -e "$HOME_DIR/.config/coppice/config.toml" ]; then
  echo "legacy config.toml was not renamed" >&2
  exit 1
fi
if [ ! -f "$HOME_DIR/.config/coppice/config.toml.imported" ]; then
  echo "missing config.toml.imported" >&2
  exit 1
fi
imported="$DATA/config.toml"
grep -F 'author_name = "Imported Ada"' "$imported" >/dev/null
grep -F 'author_email = "ada@import.example"' "$imported" >/dev/null
grep -F 'worker_count = 7' "$imported" >/dev/null
grep -F 'top_k = 4' "$imported" >/dev/null
grep -F 'command = "agent"' "$imported" >/dev/null
grep -F 'run_timeout_secs = 42' "$imported" >/dev/null
grep -F "worktrees_path = \"${DATA}/worktrees\"" "$imported" >/dev/null
if grep -E 'master_key|stolen-session|postgres://evil|enabled|default_connector|push_enabled|/tmp/evil-artifacts|^\[database\]|^\[auth\]|^\[secrets\]|^\[server\]|^\[web\]' "$imported"; then
  echo "import copied a denied setting" >&2
  exit 1
fi
if [ "$(cat "$DATA/secrets/master_key")" != "$master_before" ]; then
  echo "desktop master_key changed during import" >&2
  exit 1
fi
wait_cursor
models="$(api GET '/api/connectors/cursor/model-providers/cursor/models?refresh=true')"
assert_models_include_auto "$models"
plain="$(decrypt_token)"
if [ "$plain" != "coppice-token-still-decrypts" ]; then
  echo "forge token did not decrypt after import" >&2
  exit 1
fi
stop_server
echo "deb installed e2e PASS"
