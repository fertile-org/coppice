#!/bin/sh
# Start as root so named volumes under /data can be chowned, then drop to the
# host UID/GID so bind-mounted /repos stay owned by the operator on the host.
set -eu

uid="${COPPICE_UID:-1000}"
gid="${COPPICE_GID:-1000}"

mkdir -p /data/artifacts /data/worktrees
chown -R "${uid}:${gid}" /data/artifacts /data/worktrees

exec gosu "${uid}:${gid}" "$@"
