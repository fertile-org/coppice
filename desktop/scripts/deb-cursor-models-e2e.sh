#!/usr/bin/env bash
# Install a Coppice .deb in a clean container and list Cursor models the way a
# desktop launcher starts the app for Hung:
#   env -i HOME PATH=/usr/bin:/bin DISPLAY SHELL=/bin/zsh
# with ~/.zshrc prepending $HOME/.local/bin, and a fake `agent` there that
# prints the real `agent models` list.
#
# Usage:
#   desktop/scripts/deb-cursor-models-e2e.sh path/to/Coppice-*.deb
#
# The host needs Docker. The container defaults to ubuntu:22.04, the same
# image as the desktop CI job. COPPICE_DEB_E2E_IMAGE overrides it. Use the
# Ubuntu release that assembled the .deb when Postgres libs were copied from
# that host (a 24.04 build needs ubuntu:24.04).
#
# Asserts model-providers includes cursor, models?refresh=true returns Auto
# and the rest of the fixture, and a second refresh (after the server's 2s
# coalesce window) is a real HTTP request that runs `agent` again.
#
# The same installed binary then checks `coppice-server --version`, a fresh
# profile whose config.toml is the desktop template (no connector lines) and
# still lists Cursor Auto, and a one-time allowlist import of
# ~/.config/coppice/config.toml. That import ignores enabled = false and a
# different master_key, and a forge token still decrypts.

set -euo pipefail

if [[ "${COPPICE_DEB_E2E_INNER:-}" != "1" ]]; then
  if [[ $# -ne 1 || ! -f "$1" ]]; then
    echo "usage: $0 path/to/Coppice-*.deb" >&2
    exit 2
  fi
  if ! command -v docker >/dev/null 2>&1; then
    echo "docker is required to install the .deb in a clean container" >&2
    exit 1
  fi
  deb=$(realpath "$1")
  script=$(realpath "$0")
  image=${COPPICE_DEB_E2E_IMAGE:-ubuntu:22.04}
  commit=${COPPICE_DEB_E2E_COMMIT:-$(git rev-parse HEAD)}
  echo "deb-cursor-models-e2e: image=${image} deb=${deb} commit=${commit}"
  exec docker run --rm --shm-size=1g \
    -e COPPICE_DEB_E2E_INNER=1 \
    -e COPPICE_DEB_E2E_COMMIT="${commit}" \
    -v "${deb}:/opt/coppice-e2e/coppice.deb:ro" \
    -v "${script}:/opt/coppice-e2e/e2e.sh:ro" \
    "${image}" \
    bash /opt/coppice-e2e/e2e.sh
fi

set -m
export DEBIAN_FRONTEND=noninteractive
apt-get update
# Runtime libraries come from the .deb (tzdata, libasound2, libgbm1, GTK).
# Postgres libs are copied into the package at build time, so they are not
# installed here. These packages only let the check start a session and call
# the API: zsh and Xvfb stand in for Hung's shell and display; curl and
# python3 sign in and read JSON. python3-cryptography decrypts the forge token.
apt-get install -y --no-install-recommends \
  ca-certificates \
  curl \
  procps \
  python3 \
  python3-cryptography \
  zsh \
  xvfb \
  xauth
cp /opt/coppice-e2e/coppice.deb /tmp/file.deb
(
  cd /tmp
  apt-get install -y --no-install-recommends ./file.deb
)

if [[ ! -x /bin/zsh ]]; then
  ln -s "$(command -v zsh)" /bin/zsh
  echo "deb-cursor-models-e2e: linked /bin/zsh -> $(command -v zsh)"
fi

id hung >/dev/null 2>&1 || useradd --create-home --shell /bin/zsh hung
hung_home=$(getent passwd hung | cut -d: -f6)
install -d -o hung -g hung "${hung_home}/.local/bin" "${hung_home}/.config/Coppice"
# install -d leaves a newly created parent owned by root. ~/.local must be
# writable so the app can create ~/.local/state/Coppice/logs.
chown hung:hung "${hung_home}/.local"

# Hung's shell adds the user bin dir. Login+interactive zsh (-ilc) reads this.
cat >"${hung_home}/.zshrc" <<'EOF'
export PATH="$HOME/.local/bin:$PATH"
EOF
chown hung:hung "${hung_home}/.zshrc"

# Real `agent models` text from a Linux desktop (two spaces in "Grok 4.7  Low").
cat >"${hung_home}/.local/bin/agent" <<'EOF'
#!/bin/sh
count_file="${HOME}/.agent-models-count"
stdio_file="${HOME}/.agent-models-stdio"
lock_file="${HOME}/.agent-models-count.lock"
mkdir -p "$(dirname "$count_file")"
(
  flock 9
  n=0
  if [ -f "$count_file" ]; then
    n=$(cat "$count_file")
  fi
  n=$((n + 1))
  printf '%s\n' "$n" >"$count_file"
  if [ -t 1 ]; then
    printf 'tty\n' >>"$stdio_file"
  else
    printf 'pipe\n' >>"$stdio_file"
  fi
) 9>>"$lock_file"
if [ -f "${HOME}/.coppice-agent-empty" ]; then
  exit 0
fi
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
EOF
chmod 755 "${hung_home}/.local/bin/agent"
chown hung:hung "${hung_home}/.local/bin/agent"

# Pre-seed before first launch. ensure_config_file never overwrites an existing
# file, and the defaults for these dirs are relative to the process cwd, which
# a desktop launch cannot write. `other` omits the builtin id; the server must
# still offer `cursor`.
cat >"${hung_home}/.config/Coppice/config.toml" <<EOF
[agent]
worktrees_path = "${hung_home}/.config/Coppice/worktrees"

[agent.connectors.cursor]
enabled = true
model_providers = ["other"]

[storage]
artifacts_dir = "${hung_home}/.config/Coppice/artifacts"

[plugins]
dir = "${hung_home}/.config/Coppice/plugins"

[mcp]
builtin_plugins_dir = "${hung_home}/.config/Coppice/builtin-plugins"
EOF
chown hung:hung "${hung_home}/.config/Coppice/config.toml"

server_log="${hung_home}/.local/state/Coppice/logs/server.log"
app_log=/tmp/coppice-app.log
count_file="${hung_home}/.agent-models-count"
stdio_file="${hung_home}/.agent-models-stdio"
sandbox_note="desktop argv (no --no-sandbox)"
app_pid=""

# The Electron binary, not a `--type=` helper and not coppice-server.
hung_electron_pid() {
  local pid args
  for pid in $(ps -u hung -o pid= 2>/dev/null || true); do
    args=$(tr '\0' ' ' <"/proc/${pid}/cmdline" 2>/dev/null || true)
    case "${args}" in
      /opt/Coppice/coppice\ --type=*) continue ;;
      /opt/Coppice/coppice | /opt/Coppice/coppice\ *)
        printf '%s\n' "${pid}"
        return 0
        ;;
    esac
  done
  return 1
}

hung_running() {
  pgrep -u hung -x coppice >/dev/null 2>&1 \
    || pgrep -u hung -x coppice-server >/dev/null 2>&1 \
    || pgrep -u hung -x postgres >/dev/null 2>&1
}

stop_app() {
  local i electron_pid
  if [[ -z "${app_pid}" ]]; then
    return 0
  fi
  # Signal Electron itself and leave Xvfb up. A group SIGTERM kills the
  # display at the same time; Ozone then aborts with "Failed to shutdown"
  # before before-quit can stop the detached coppice-server. That server
  # keeps the profile, and the next launch cannot create SingletonLock.
  electron_pid=$(hung_electron_pid || true)
  if [[ -n "${electron_pid}" ]]; then
    kill -TERM "${electron_pid}" 2>/dev/null || true
  else
    kill -TERM "${app_pid}" 2>/dev/null || true
  fi
  for i in $(seq 1 25); do
    if ! kill -0 "${app_pid}" 2>/dev/null && ! hung_running; then
      break
    fi
    sleep 1
  done
  kill -TERM "-${app_pid}" 2>/dev/null || true
  pkill -TERM -u hung -x coppice-server 2>/dev/null || true
  pkill -TERM -u hung -x postgres 2>/dev/null || true
  for i in $(seq 1 10); do
    if ! kill -0 "${app_pid}" 2>/dev/null && ! hung_running; then
      break
    fi
    sleep 1
  done
  kill -KILL "-${app_pid}" 2>/dev/null || true
  pkill -KILL -u hung -x coppice 2>/dev/null || true
  pkill -KILL -u hung -x coppice-server 2>/dev/null || true
  pkill -KILL -u hung -x postgres 2>/dev/null || true
  wait "${app_pid}" 2>/dev/null || true
  app_pid=""
  if [[ -d "${hung_home}/.config/Coppice" ]]; then
    chown -R hung:hung "${hung_home}/.config/Coppice" || true
    rm -f "${hung_home}/.config/Coppice/SingletonLock" \
      "${hung_home}/.config/Coppice/SingletonCookie" \
      "${hung_home}/.config/Coppice/SingletonSocket"
  fi
}
trap stop_app EXIT

launch_app() {
  stop_app
  {
    echo "----- launch${*:+ $*} -----"
  } >>"${app_log}"
  # env -i drops XAUTHORITY, so the X server is started with access control off.
  # The app process still receives only HOME, PATH, DISPLAY, and SHELL.
  runuser -u hung -- xvfb-run -a -s "-screen 0 1280x1024x24 -ac" bash -c \
    'exec env -i HOME="$HOME" PATH=/usr/bin:/bin DISPLAY="$DISPLAY" SHELL=/bin/zsh /opt/Coppice/coppice "$@"' \
    bash "$@" >>"${app_log}" 2>&1 &
  app_pid=$!
}

wait_ready() {
  local i
  for i in $(seq 1 90); do
    if [[ -f "${server_log}" ]] && grep -q 'COPPICE_READY url=http://127.0.0.1:' "${server_log}"; then
      return 0
    fi
    if [[ -f "${server_log}" ]] && grep -q 'coppice-server desktop: failed' "${server_log}"; then
      return 1
    fi
    if ! kill -0 "${app_pid}" 2>/dev/null; then
      return 1
    fi
    sleep 1
  done
  return 1
}

dump_failure() {
  echo "----- app log -----" >&2
  tail -n 80 "${app_log}" >&2 || true
  echo "----- server log -----" >&2
  tail -n 120 "${server_log}" >&2 || true
  echo "----- processes -----" >&2
  ps -u hung -o pid,user,args >&2 || true
  echo "----- profile -----" >&2
  ls -ld "${hung_home}/.config" "${hung_home}/.config/Coppice" \
    "${hung_home}/.local" "${hung_home}/.local/state/Coppice/logs" >&2 || true
}

assert_installed_version() {
  local listed version_out commit_line
  listed=$(dpkg -L coppice-desktop | grep -x '/opt/Coppice/resources/bin/coppice-server' || true)
  if [[ "${listed}" != "/opt/Coppice/resources/bin/coppice-server" || ! -x "${listed}" ]]; then
    echo "dpkg -L coppice-desktop did not list the real coppice-server" >&2
    dpkg -L coppice-desktop | grep coppice-server || true
    exit 1
  fi
  if [[ -z "${COPPICE_DEB_E2E_COMMIT:-}" ]]; then
    echo "COPPICE_DEB_E2E_COMMIT is unset" >&2
    exit 1
  fi
  version_out="$(env -i PATH=/usr/bin:/bin HOME="${hung_home}" "${listed}" --version)"
  printf '%s\n' "${version_out}"
  commit_line="$(printf '%s\n' "${version_out}" | sed -n '2p')"
  if [[ "${commit_line}" != "commit ${COPPICE_DEB_E2E_COMMIT}" ]]; then
    echo "expected commit ${COPPICE_DEB_E2E_COMMIT}, got: ${commit_line}" >&2
    exit 1
  fi
  case "$(printf '%s\n' "${version_out}" | sed -n '1p')" in
    coppice-server\ *) ;;
    *)
      echo "version line is not coppice-server <version>" >&2
      exit 1
      ;;
  esac
}

open_session() {
  headers=$(mktemp)
  session_body=$(mktemp)
  curl -sS -D "${headers}" -o "${session_body}" -X POST "${base_url}/api/auth/desktop-session"
  token=$(python3 - "${headers}" <<'PY'
import sys
token = ""
for line in open(sys.argv[1], encoding="utf-8", errors="replace"):
    if line.lower().startswith("set-cookie:"):
        pair = line.split(":", 1)[1].strip().split(";", 1)[0]
        name, _, value = pair.partition("=")
        if name.strip() == "coppice_session" and value:
            token = value
            break
if not token:
    sys.exit("desktop-session did not set coppice_session")
print(token)
PY
)
  csrf=$(python3 - "${session_body}" <<'PY'
import json, sys
body = json.load(open(sys.argv[1], encoding="utf-8"))
token = body.get("csrfToken") or ""
if not token:
    sys.exit("desktop-session did not return csrfToken")
print(token)
PY
)
}

: >"${app_log}"
assert_installed_version
launch_app
if ! wait_ready; then
  if grep -Eqi 'No usable sandbox|chrome-sandbox|Failed to move to new namespace|SUID sandbox' "${app_log}" 2>/dev/null; then
    echo "deb-cursor-models-e2e: sandbox refused the first launch; retrying with --no-sandbox"
    sandbox_note="relaunched with --no-sandbox (container refused the SUID sandbox)"
    launch_app --no-sandbox
    if ! wait_ready; then
      dump_failure
      echo "deb-cursor-models-e2e: installed app did not become ready" >&2
      exit 1
    fi
  else
    dump_failure
    echo "deb-cursor-models-e2e: installed app did not become ready" >&2
    exit 1
  fi
fi

ready_line=$(grep -o 'COPPICE_READY url=http://127.0.0.1:[0-9]*' "${server_log}" | tail -n 1)
base_url=${ready_line#COPPICE_READY url=}
echo "deb-cursor-models-e2e: ready ${base_url}"

# Startup probes `agent models` once. Wait until that count stops moving.
count_now() {
  if [[ -f "${count_file}" ]]; then
    cat "${count_file}"
  else
    echo 0
  fi
}
stable=""
last=$(count_now)
unchanged=0
for _ in $(seq 1 45); do
  sleep 1
  now=$(count_now)
  if [[ "${now}" == "${last}" ]]; then
    unchanged=$((unchanged + 1))
    if [[ "${unchanged}" -ge 3 ]]; then
      stable=${now}
      break
    fi
  else
    unchanged=0
    last=${now}
  fi
done
if [[ -z "${stable}" ]]; then
  dump_failure
  echo "deb-cursor-models-e2e: agent invocation count did not settle (last=${last})" >&2
  exit 1
fi
baseline=${stable}

open_session

cookie_jar=$(mktemp)
printf 'coppice_session=%s\n' "${token}" >"${cookie_jar}"
# curl cookie jar needs the Netscape header when using -b file of name=value
# pairs; a raw Cookie header avoids writing the token into a second format.
fetch() {
  local out=$1
  shift
  curl -sS -o "${out}" -w '%{http_code}' -H "Cookie: coppice_session=${token}" "$@"
}

providers_body=$(mktemp)
providers_code=$(fetch "${providers_body}" "${base_url}/api/connectors/cursor/model-providers")
python3 - "${providers_body}" "${providers_code}" <<'PY'
import json, sys
body, code = sys.argv[1], sys.argv[2]
if code != "200":
    sys.exit(f"model-providers HTTP {code}: {open(body).read()}")
ids = [item["id"] for item in json.load(open(body))["items"]]
missing = [provider for provider in ("other", "cursor") if provider not in ids]
if missing:
    sys.exit(f"model-providers missing {missing}: {ids}")
print("providers:", ", ".join(ids))
PY

# Past the 2s model-list coalesce so each refresh runs `agent` again.
sleep 3

refresh() {
  local out=$1
  fetch "${out}" "${base_url}/api/connectors/cursor/model-providers/cursor/models?refresh=true"
}

wait_count_at_least() {
  local want=$1
  local i now
  for i in $(seq 1 40); do
    now=$(count_now)
    if [[ "${now}" -ge "${want}" ]]; then
      echo "${now}"
      return 0
    fi
    sleep 1
  done
  echo "${now}"
  return 1
}

models_body=$(mktemp)
models_code=$(refresh "${models_body}")
python3 - "${models_body}" "${models_code}" <<'PY'
import json, sys
body, code = sys.argv[1], sys.argv[2]
if code != "200":
    sys.exit(f"models refresh HTTP {code}: {open(body).read()}")
items = {item["id"]: item["name"] for item in json.load(open(body))["items"]}
expected = {
    "auto": "Auto",
    "gpt-5.3-codex-low": "Codex 5.3 Low",
    "composer-2.5": "Composer 2.5",
    "claude-opus-5-thinking-high": "Claude Opus 5 1M Thinking",
    "claude-fable-5-thinking-high": "Claude Fable 5 1M Thinking (NO ZDR)",
    "grok-4.7-low": "Grok 4.7  Low",
    "cursor-grok-4.6-low-fast": "Grok 4.6 Low Fast",
}
bad = [f"{model_id}={items.get(model_id)!r}" for model_id, name in expected.items() if items.get(model_id) != name]
if bad:
    sys.exit("models mismatch: " + ", ".join(bad))
print("models:", ", ".join(f"{model_id}={name}" for model_id, name in items.items()))
PY

if ! after_first=$(wait_count_at_least $((baseline + 1))); then
  dump_failure
  echo "deb-cursor-models-e2e: first refresh did not run agent (baseline=${baseline} count=${after_first})" >&2
  exit 1
fi

sleep 3
models_body_2=$(mktemp)
models_code_2=$(refresh "${models_body_2}")
if [[ "${models_code_2}" != "200" ]]; then
  echo "deb-cursor-models-e2e: second refresh HTTP ${models_code_2}: $(cat "${models_body_2}")" >&2
  dump_failure
  exit 1
fi
if ! after_second=$(wait_count_at_least $((after_first + 1))); then
  dump_failure
  echo "deb-cursor-models-e2e: second refresh did not run agent (after first=${after_first} count=${after_second})" >&2
  exit 1
fi

python3 - "${stdio_file}" <<'PY'
import sys
lines = [line for line in open(sys.argv[1], encoding="utf-8") if line.strip()]
if not lines or any(line.strip() != "pipe" for line in lines):
    sys.exit("agent stdout was not a pipe: " + repr(lines))
print(f"stdio: pipe ({len(lines)} invocations)")
PY

python3 - "${server_log}" "${token}" <<'PY'
import sys
log = open(sys.argv[1], encoding="utf-8", errors="replace").read()
token = sys.argv[2]
models_path = "/api/connectors/cursor/model-providers/cursor/models"
refresh = [
    line for line in log.splitlines()
    if "http request" in line and models_path in line
]
if len(refresh) < 2:
    sys.exit(f"expected two models request logs, found {len(refresh)}")
query_lines = [line for line in refresh if "?" in line or "refresh=" in line]
if query_lines:
    sys.exit("request log included a query string: " + repr(query_lines))
if token and token in log:
    sys.exit("server log contains the session token")
for secret in ("cookie", "authorization", "coppice_session"):
    for line in refresh:
        if secret in line.lower():
            sys.exit(f"request log line mentions {secret}: {line}")
fallback = "yes" if "login shell capture fell back" in log else "no"
program = "missing"
for line in log.splitlines():
    if "listed models" in line and ".local/bin/agent" in line:
        program = line
        break
if program == "missing":
    sys.exit("listed-models log did not name .local/bin/agent")
print(f"refresh_requests: {len(refresh)}")
print(f"shell_fallback: {fallback}")
print(f"program: {program}")
PY

# A missing route is still an info line: method, path, status, no query string.
missing_body=$(mktemp)
missing_code=$(fetch "${missing_body}" "${base_url}/api/this-route-is-missing")
if [[ "${missing_code}" != "404" ]]; then
  echo "missing route status ${missing_code}" >&2
  exit 1
fi
python3 - "${server_log}" <<'PY'
import sys
path = "/api/this-route-is-missing"
lines = [
    line for line in open(sys.argv[1], encoding="utf-8", errors="replace")
    if "http request" in line and path in line
]
if not lines:
    sys.exit("404 was not logged")
for line in lines:
    if "?" in line or "refresh=" in line:
        sys.exit("404 request log included a query string: " + line)
    if "status=404" not in line and "status: 404" not in line:
        sys.exit("404 request log missing status: " + line)
print("logged 404:", path)
PY

launch_known() {
  if [[ "${sandbox_note}" == *--no-sandbox* ]]; then
    launch_app --no-sandbox
  else
    launch_app
  fi
}

read_ready() {
  ready_line=$(grep -o 'COPPICE_READY url=http://127.0.0.1:[0-9]*' "${server_log}" | tail -n 1)
  base_url=${ready_line#COPPICE_READY url=}
  echo "deb-cursor-models-e2e: ready ${base_url}"
}

relaunch() {
  stop_app
  rm -f "${server_log}"
  launch_known
  if ! wait_ready; then
    dump_failure
    echo "deb-cursor-models-e2e: relaunch did not become ready" >&2
    exit 1
  fi
  read_ready
  open_session
}

assert_auto() {
  python3 - "$1" "$2" <<'PY'
import json, sys
body, code = sys.argv[1], sys.argv[2]
if code != "200":
    sys.exit(f"models HTTP {code}: {open(body).read()}")
items = json.load(open(body))["items"]
if not any(item.get("id") == "auto" and item.get("name") == "Auto" for item in items):
    sys.exit("Auto missing: " + open(body).read())
print("auto: Auto")
PY
}

poll_auto() {
  local i body code
  body=$(mktemp)
  code=""
  for i in $(seq 1 45); do
    code=$(fetch "${body}" "${base_url}/api/connectors/cursor/model-providers/cursor/models?refresh=true")
    if [[ "${code}" == "200" ]]; then
      assert_auto "${body}" "${code}"
      return 0
    fi
    sleep 1
  done
  echo "Cursor models stayed HTTP ${code}: $(cat "${body}")" >&2
  dump_failure
  exit 1
}

post_json() {
  local out=$1 path=$2 body=$3
  curl -sS -o "${out}" -w '%{http_code}' \
    -H "Cookie: coppice_session=${token}" \
    -H "X-CSRF-Token: ${csrf}" \
    -H 'Content-Type: application/json' \
    -X POST "${base_url}${path}" \
    -d "${body}"
}

decrypt_forge_token() {
  local port hex_ct hex_nonce
  port="$(sed -n '4p' "${hung_home}/.config/Coppice/pg/data/postmaster.pid")"
  read -r hex_ct hex_nonce < <(
    PGPASSWORD="$(cat "${hung_home}/.config/Coppice/secrets/pg_password")" \
      LD_LIBRARY_PATH="/opt/Coppice/resources/postgres/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
      /opt/Coppice/resources/postgres/bin/psql \
      -h 127.0.0.1 -p "${port}" -U coppice -d coppice -At -c \
      "SELECT encode(ciphertext,'hex') || ' ' || encode(nonce,'hex') FROM secrets WHERE name LIKE 'repo-forge-token-%' LIMIT 1"
  )
  python3 - "${hung_home}/.config/Coppice/secrets/master_key" "${hex_ct}" "${hex_nonce}" <<'PY'
import hashlib, sys
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
master = open(sys.argv[1], encoding="utf-8").read().strip()
ciphertext = bytes.fromhex(sys.argv[2])
nonce = bytes.fromhex(sys.argv[3])
sys.stdout.write(AESGCM(hashlib.sha256(master.encode()).digest()).decrypt(nonce, ciphertext, None).decode())
PY
}

echo "fresh profile"
stop_app
rm -rf "${hung_home}/.config/Coppice" "${hung_home}/.config/coppice"
# The first launch had this directory. Electron cannot create SingletonLock
# when the profile dir is missing or not writable by hung.
install -d -o hung -g hung "${hung_home}/.config/Coppice"
relaunch

expected_config=$(mktemp)
cat >"${expected_config}" <<EOF
# Coppice desktop configuration. Edits here survive upgrades.
# Secrets (session secret, encryption key, passwords) live in secrets/, not here.
# The server port, database URL and auth mode are set by the desktop app at startup.
# Turn connectors on or off in Connectors in the app.

# How agents run.
[agent]
# Coppice gives each ticket its own git worktree in this folder.
worktrees_path = "${hung_home}/.config/Coppice/worktrees"

# Logs and other files from agent runs and chats.
[storage]
artifacts_dir = "${hung_home}/.config/Coppice/artifacts"

# Your default plugin folder. Coppice always looks here for plugins.
[plugins]
# Add more folders on the Plugins page. This one can't be removed there.
dir = "${hung_home}/.config/Coppice/plugins"

# Skills that ship with Coppice.
[mcp]
# Coppice rewrites coppice/skills here on every start. Edits there are lost.
builtin_plugins_dir = "${hung_home}/.config/Coppice/builtin-plugins"
EOF
if ! diff -u "${expected_config}" "${hung_home}/.config/Coppice/config.toml"; then
  echo "fresh config.toml is not the desktop template" >&2
  exit 1
fi
if grep -q 'default_connector\|\[agent\.connectors' "${hung_home}/.config/Coppice/config.toml"; then
  echo "fresh template contains connector settings" >&2
  exit 1
fi
poll_auto

repo=/tmp/coppice-e2e-repo
rm -rf "${repo}"
git init -q -b main "${repo}"
chmod -R a+rX "${repo}"
repo_body=$(mktemp)
repo_code=$(post_json "${repo_body}" /api/repos "{\"name\":\"e2e\",\"localPath\":\"${repo}\",\"defaultBranch\":\"main\"}")
if [[ "${repo_code}" != "201" ]]; then
  echo "create repo HTTP ${repo_code}: $(cat "${repo_body}")" >&2
  exit 1
fi
repo_id=$(python3 - "${repo_body}" <<'PY'
import json, sys
repo_id = json.load(open(sys.argv[1])).get("id")
if not repo_id:
    sys.exit("repo response has no id")
print(repo_id)
PY
)
token_body=$(mktemp)
token_code=$(post_json "${token_body}" "/api/repos/${repo_id}/forge-token" '{"token":"coppice-token-still-decrypts"}')
if [[ "${token_code}" != "200" ]]; then
  echo "forge token HTTP ${token_code}: $(cat "${token_body}")" >&2
  exit 1
fi
plain="$(decrypt_forge_token)"
if [[ "${plain}" != "coppice-token-still-decrypts" ]]; then
  echo "forge token did not decrypt before import" >&2
  exit 1
fi
master_before="$(cat "${hung_home}/.config/Coppice/secrets/master_key")"

sleep 3
touch "${hung_home}/.coppice-agent-empty"
empty_body=$(mktemp)
empty_code=$(fetch "${empty_body}" "${base_url}/api/connectors/cursor/model-providers/cursor/models?refresh=true")
rm -f "${hung_home}/.coppice-agent-empty"
if [[ "${empty_code}" != "502" ]]; then
  echo "empty model list status ${empty_code}: $(cat "${empty_body}")" >&2
  exit 1
fi
if ! grep -E 'returned no models|listed no models' "${empty_body}" >/dev/null; then
  echo "empty model list was not reported as a failure: $(cat "${empty_body}")" >&2
  exit 1
fi

echo "legacy import"
mkdir -p "${hung_home}/.config/coppice"
cat >"${hung_home}/.config/coppice/config.toml" <<'EOF'
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
chown -R hung:hung "${hung_home}/.config/coppice"
relaunch

notice_body=$(mktemp)
notice_code=$(fetch "${notice_body}" "${base_url}/api/import-notice")
python3 - "${notice_body}" "${notice_code}" <<'PY'
import json, sys
body, code = sys.argv[1], sys.argv[2]
if code != "200":
    sys.exit(f"import-notice HTTP {code}: {open(body).read()}")
want = "Imported your git author, worker, knowledge and connector settings from `~/.config/coppice/config.toml`. Connectors stay as set in Connectors. The old file is now `config.toml.imported`."
got = json.load(open(body)).get("message")
if got != want:
    sys.exit("import notice:\n" + repr(got))
print("import notice ok")
PY
if [[ -e "${hung_home}/.config/coppice/config.toml" ]]; then
  echo "legacy config.toml was not renamed" >&2
  exit 1
fi
if [[ ! -f "${hung_home}/.config/coppice/config.toml.imported" ]]; then
  echo "missing config.toml.imported" >&2
  exit 1
fi
imported="${hung_home}/.config/Coppice/config.toml"
grep -F 'author_name = "Imported Ada"' "${imported}" >/dev/null
grep -F 'author_email = "ada@import.example"' "${imported}" >/dev/null
grep -F 'worker_count = 7' "${imported}" >/dev/null
grep -F 'top_k = 4' "${imported}" >/dev/null
grep -F 'command = "agent"' "${imported}" >/dev/null
grep -F 'run_timeout_secs = 42' "${imported}" >/dev/null
grep -F "worktrees_path = \"${hung_home}/.config/Coppice/worktrees\"" "${imported}" >/dev/null
if grep -E 'master_key|stolen-session|postgres://evil|enabled|default_connector|push_enabled|/tmp/evil-artifacts|^\[database\]|^\[auth\]|^\[secrets\]|^\[server\]|^\[web\]' "${imported}"; then
  echo "import copied a denied setting" >&2
  exit 1
fi
if [[ "$(cat "${hung_home}/.config/Coppice/secrets/master_key")" != "${master_before}" ]]; then
  echo "desktop master_key changed during import" >&2
  exit 1
fi
poll_auto
plain="$(decrypt_forge_token)"
if [[ "${plain}" != "coppice-token-still-decrypts" ]]; then
  echo "forge token did not decrypt after import" >&2
  exit 1
fi

echo "deb-cursor-models-e2e: ok"
echo "  sandbox: ${sandbox_note}"
echo "  invocations: baseline=${baseline} after_first=${after_first} after_second=${after_second}"
echo "  second refresh ran agent ($((after_second - after_first)) new invocation(s)) and was logged"
