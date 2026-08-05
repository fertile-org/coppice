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

exec gosu "${uid}:${gid}" "$@"
