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
  echo "deb-cursor-models-e2e: image=${image} deb=${deb}"
  exec docker run --rm --shm-size=1g \
    -e COPPICE_DEB_E2E_INNER=1 \
    -v "${deb}:/opt/coppice-e2e/coppice.deb:ro" \
    -v "${script}:/opt/coppice-e2e/e2e.sh:ro" \
    "${image}" \
    bash /opt/coppice-e2e/e2e.sh
fi

set -m
export DEBIAN_FRONTEND=noninteractive
# Bundled Postgres is built --with-system-tzdata=/usr/share/zoneinfo. Without
# tzdata, connecting with TimeZone=UTC fails.
apt-get update
apt-get install -y --no-install-recommends \
  ca-certificates \
  curl \
  python3 \
  tzdata \
  xz-utils \
  zsh \
  xvfb \
  xauth \
  libxml2 \
  libxslt1.1 \
  libreadline8 \
  libxxhash0 \
  liblz4-1 \
  libzstd1 \
  libssl3 \
  libgssapi-krb5-2 \
  libgbm1 \
  /opt/coppice-e2e/coppice.deb
# Electron needs libasound.so.2. The .deb depends on libasound2 (Ubuntu 22.04).
# 24.04 renamed that library to libasound2t64, so install whichever this image has.
if apt-cache show libasound2t64 >/dev/null 2>&1; then
  apt-get install -y --no-install-recommends libasound2t64
else
  apt-get install -y --no-install-recommends libasound2
fi

if [[ ! -x /bin/zsh ]]; then
  ln -s "$(command -v zsh)" /bin/zsh
  echo "deb-cursor-models-e2e: linked /bin/zsh -> $(command -v zsh)"
fi

id hung >/dev/null 2>&1 || useradd --create-home --shell /bin/zsh hung
hung_home=$(getent passwd hung | cut -d: -f6)
install -d -o hung -g hung "${hung_home}/.local/bin" "${hung_home}/.config/Coppice"

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

server_log="${hung_home}/.config/Coppice/logs/server.log"
app_log=/tmp/coppice-app.log
count_file="${hung_home}/.agent-models-count"
stdio_file="${hung_home}/.agent-models-stdio"
sandbox_note="desktop argv (no --no-sandbox)"
app_pid=""

stop_app() {
  if [[ -n "${app_pid}" ]]; then
    kill -TERM "-${app_pid}" 2>/dev/null || kill -TERM "${app_pid}" 2>/dev/null || true
    wait "${app_pid}" 2>/dev/null || true
    app_pid=""
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
}

: >"${app_log}"
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

echo "deb-cursor-models-e2e: ok"
echo "  sandbox: ${sandbox_note}"
echo "  invocations: baseline=${baseline} after_first=${after_first} after_second=${after_second}"
echo "  second refresh ran agent ($((after_second - after_first)) new invocation(s)) and was logged"
