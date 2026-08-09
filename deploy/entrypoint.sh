#!/bin/sh
# Start as root so named volumes under /data can be chowned, then drop to the
# host UID/GID so bind-mounted /repos stay owned by the operator on the host.
set -eu

uid="${COPPICE_UID:-1000}"
gid="${COPPICE_GID:-1000}"
config="${COPPICE_CONFIG:-/etc/coppice/config.toml}"

# Compose bind-mounts deploy/config/config.toml here. If that host path was
# missing, Docker creates a directory instead — opening it as a file fails with
# a cryptic "No such file or directory".
if [ -d "$config" ]; then
  echo "error: $config is a directory, expected a file." >&2
  echo "On the host, remove it and copy the example:" >&2
  echo "  rm -rf deploy/config/config.toml" >&2
  echo "  cp deploy/config/config.example.toml deploy/config/config.toml" >&2
  echo "Then: docker compose -f deploy/docker-compose.yml up -d --force-recreate server" >&2
  exit 1
fi
if [ ! -f "$config" ]; then
  echo "error: config file not found: $config" >&2
  echo "On the host: cp deploy/config/config.example.toml deploy/config/config.toml" >&2
  exit 1
fi

mkdir -p /data/artifacts /data/worktrees
chown -R "${uid}:${gid}" /data/artifacts /data/worktrees

# gosu may reset HOME from /etc/passwd. For numeric COPPICE_UID with no
# passwd entry that clears Compose HOME, connector auth under $HOME would
# be invisible to API-spawned CLIs. Preserve Compose HOME/PATH.
preserved_home="${HOME:-}"
preserved_path="${PATH:-}"

# Managed connector home (Compose volume). Ensure ownership for COPPICE_UID.
if [ -n "$preserved_home" ] && [ -d "$preserved_home" ]; then
  mkdir -p "$preserved_home"
  chown "${uid}:${gid}" "$preserved_home"
fi

if [ -n "$preserved_home" ] && [ -n "$preserved_path" ]; then
  exec gosu "${uid}:${gid}" env HOME="$preserved_home" PATH="$preserved_path" "$@"
fi
if [ -n "$preserved_home" ]; then
  exec gosu "${uid}:${gid}" env HOME="$preserved_home" "$@"
fi
exec gosu "${uid}:${gid}" "$@"
