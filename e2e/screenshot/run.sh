#!/bin/sh
# Start, reuse, or tear down the screenshot stack.
# The private directory is recorded in e2e/screenshot/.capture-dir (mode 0600).
# `up` leaves the stack and directory for `docs`. `down` removes both.
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"
pointer=$root/e2e/screenshot/.capture-dir
cmd=${1:-up}

if [ -L "$pointer" ]; then
  printf '%s\n' "refusing symlink capture pointer: $pointer" >&2
  exit 1
fi

pointer_value() {
  key=$1
  sed -n "s/^${key}='\\([^']*\\)'\$/\\1/p" "$pointer" | head -n 1
}

load_pointer() {
  if [ ! -f "$pointer" ]; then
    printf '%s\n' "no capture directory is recorded. Run make screenshot first." >&2
    exit 1
  fi
  COPPICE_SCREENSHOT_DIR=$(pointer_value COPPICE_SCREENSHOT_DIR)
  COPPICE_REPOS_HOST=$(pointer_value COPPICE_REPOS_HOST)
  COPPICE_SCREENSHOT_CONFIG=$(pointer_value COPPICE_SCREENSHOT_CONFIG)
  COPPICE_SCREENSHOT_TRANSFER=$(pointer_value COPPICE_SCREENSHOT_TRANSFER)
  if [ -z "$COPPICE_SCREENSHOT_DIR" ] || [ -z "$COPPICE_REPOS_HOST" ] || [ -z "$COPPICE_SCREENSHOT_CONFIG" ] || [ -z "$COPPICE_SCREENSHOT_TRANSFER" ]; then
    printf '%s\n' "capture pointer is incomplete: $pointer" >&2
    exit 1
  fi
  case "$COPPICE_SCREENSHOT_TRANSFER" in
    bind|copy) ;;
    *)
      printf '%s\n' "capture pointer has an invalid transfer mode" >&2
      exit 1
      ;;
  esac
  if [ -L "$COPPICE_SCREENSHOT_CONFIG" ] || [ ! -f "$COPPICE_SCREENSHOT_CONFIG" ]; then
    printf '%s\n' "capture config is missing or is a symlink: $COPPICE_SCREENSHOT_CONFIG" >&2
    exit 1
  fi
  if [ -L "$COPPICE_REPOS_HOST" ] || [ ! -d "$COPPICE_REPOS_HOST" ]; then
    printf '%s\n' "capture repos directory is missing or is a symlink: $COPPICE_REPOS_HOST" >&2
    exit 1
  fi
  export COPPICE_REPOS_HOST COPPICE_SCREENSHOT_CONFIG COPPICE_SCREENSHOT_DIR COPPICE_SCREENSHOT_TRANSFER
}

write_pointer() {
  dir=$1
  transfer=${COPPICE_SCREENSHOT_TRANSFER:-bind}
  case "$transfer" in
    bind|copy) ;;
    *)
      printf '%s\n' "COPPICE_SCREENSHOT_TRANSFER must be bind or copy" >&2
      exit 1
      ;;
  esac
  tmp=$(mktemp "$root/e2e/screenshot/.capture-dir.XXXXXX")
  chmod 600 "$tmp"
  cat > "$tmp" <<EOF
COPPICE_SCREENSHOT_DIR='$dir'
COPPICE_REPOS_HOST='$dir/repos'
COPPICE_SCREENSHOT_CONFIG='$dir/config.toml'
COPPICE_SCREENSHOT_TRANSFER='$transfer'
EOF
  mv "$tmp" "$pointer"
  chmod 600 "$pointer"
}

compose() {
  # Do not pass a host SECRETS_MASTER_KEY into this process.
  unset SECRETS_MASTER_KEY || true
  overlay="-f deploy/docker-compose.screenshot.yml"
  if [ "${COPPICE_SCREENSHOT_TRANSFER:-bind}" = copy ]; then
    overlay="$overlay -f deploy/docker-compose.screenshot-copy.yml"
  elif [ "${COPPICE_SCREENSHOT_TRANSFER:-bind}" != bind ]; then
    printf '%s\n' "COPPICE_SCREENSHOT_TRANSFER must be bind or copy" >&2
    exit 1
  fi
  # shellcheck disable=SC2086
  COPPICE_REPOS_HOST="$COPPICE_REPOS_HOST" COPPICE_SCREENSHOT_CONFIG="$COPPICE_SCREENSHOT_CONFIG" \
    ${DOCKER_COMPOSE:-docker compose} -f deploy/docker-compose.yml -p coppice-screenshot $overlay "$@"
}

publish_config() {
  if [ -L "$COPPICE_SCREENSHOT_CONFIG" ] || [ ! -f "$COPPICE_SCREENSHOT_CONFIG" ]; then
    printf '%s\n' "refusing to copy screenshot config: $COPPICE_SCREENSHOT_CONFIG" >&2
    return 1
  fi
  compose cp "$COPPICE_SCREENSHOT_CONFIG" server:/etc/coppice/config.toml
}

cleanup_files() {
  if [ -n "${dir:-}" ]; then
    sh "$root/e2e/screenshot/remove-capture.sh" "$dir" || true
  fi
  rm -f "$pointer"
}

drop_stack() {
  compose down -v --remove-orphans || true
  cleanup_files
}

start_stack() {
  if [ -f "$pointer" ]; then
    old=$(pointer_value COPPICE_SCREENSHOT_DIR)
    if [ -n "$old" ]; then
      sh "$root/e2e/screenshot/remove-capture.sh" "$old" || true
    fi
    rm -f "$pointer"
  fi
  dir=$(mktemp -d "${TMPDIR:-/tmp}/coppice-screenshot.XXXXXX")
  chmod 700 "$dir"
  if ! sh "$root/e2e/screenshot/prepare-capture.sh" "$dir" "$root/deploy/config/config.example.toml"; then
    sh "$root/e2e/screenshot/remove-capture.sh" "$dir" || rm -rf "$dir"
    exit 1
  fi
  write_pointer "$dir"
  export COPPICE_REPOS_HOST=$dir/repos
  export COPPICE_SCREENSHOT_CONFIG=$dir/config.toml
  export COPPICE_SCREENSHOT_DIR=$dir
  set +e
  node "$root/e2e/screenshot/check-compose.mjs"
  status=$?
  set -e
  if [ "$status" -ne 0 ]; then
    cleanup_files
    exit "$status"
  fi
  if ! compose down -v --remove-orphans; then
    drop_stack
    exit 1
  fi
  if [ "${COPPICE_SCREENSHOT_TRANSFER:-bind}" = copy ]; then
    # Create every container stopped, install the private config, then start.
    # A recreate after the copy would discard that file.
    if ! compose up -d --build --no-start; then
      drop_stack
      exit 1
    fi
    if ! publish_config; then
      drop_stack
      exit 1
    fi
    if ! compose up -d --no-build --no-recreate; then
      drop_stack
      exit 1
    fi
  else
    if ! compose up -d --build; then
      drop_stack
      exit 1
    fi
    if ! compose up -d --force-recreate --no-deps server; then
      drop_stack
      exit 1
    fi
  fi
}

case "$cmd" in
  up)
    start_stack
    cd e2e && yarn install --frozen-lockfile && yarn playwright install chromium
    cd "$root"
    if ! COPPICE_SCREENSHOT_CONFIG="$COPPICE_SCREENSHOT_CONFIG" node e2e/screenshot/capture-hero.mjs; then
      printf '%s\n' "screenshot: capture failed. The loopback stack is still up. Run make screenshot-down to remove it." >&2
      exit 1
    fi
    if ! COPPICE_SCREENSHOT_CONFIG="$COPPICE_SCREENSHOT_CONFIG" node e2e/screenshot/capture-docs.mjs; then
      printf '%s\n' "screenshot: capture failed. The loopback stack is still up. Run make screenshot-down to remove it." >&2
      exit 1
    fi
    printf '%s\n' "screenshot: stack is up. Private dir $dir"
    printf '%s\n' "screenshot: run make screenshot-down when the stills are saved."
    ;;
  stack)
    start_stack
    printf '%s\n' "screenshot: stack is up. Private dir $dir"
    printf '%s\n' "screenshot: run make screenshot-down when the stills are saved."
    ;;
  docs)
    load_pointer
    cd e2e && yarn install --frozen-lockfile && yarn playwright install chromium
    cd "$root"
    COPPICE_SCREENSHOT_CONFIG="$COPPICE_SCREENSHOT_CONFIG" node e2e/screenshot/capture-docs.mjs
    ;;
  articles)
    load_pointer
    cd e2e && yarn install --frozen-lockfile && yarn playwright install chromium
    cd "$root"
    COPPICE_SCREENSHOT_CONFIG="$COPPICE_SCREENSHOT_CONFIG" node e2e/screenshot/capture-articles.mjs
    ;;
  down)
    if [ -f "$pointer" ]; then
      load_pointer
      compose down -v --remove-orphans || true
      sh "$root/e2e/screenshot/remove-capture.sh" "$COPPICE_SCREENSHOT_DIR" || true
      rm -f "$pointer"
    else
      printf '%s\n' "screenshot: no private directory is recorded, so no stack was stopped."
    fi
    ;;
  *)
    printf '%s\n' "usage: run.sh up|stack|docs|articles|down" >&2
    exit 1
    ;;
esac
