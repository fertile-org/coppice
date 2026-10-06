#!/bin/sh
# Create the marketing git repo and ticket worktree inside the server container.
# Args: <ticket-short-id> <repo-slug>
set -eu

short="$1"
slug="$2"
repo=/tmp/coppice-screenshot-repo
wt="/data/worktrees/TICKET-${short}-${slug}"
branch="agent/TICKET-${short}"

rm -rf "$repo" "$wt"
mkdir -p "$repo/desktop/src" "$repo/server/src" "$repo/docs"

git init -b main "$repo" >/dev/null
git -C "$repo" config user.email screenshot@coppice.local
git -C "$repo" config user.name "Coppice Screenshot"

cat > "$repo/desktop/src/postgres.ts" <<'EOF'
/** Desktop database bootstrap. */
export function postgresPort(): number {
  return 5432;
}

export function dataDir(home: string): string {
  return `${home}/Library/Application Support/Coppice/postgres`;
}
EOF

cat > "$repo/server/src/desktop_bootstrap.rs" <<'EOF'
/// App data directory used by `coppice-server desktop`.
pub fn data_dir() -> &'static str {
    "coppice-data"
}

pub fn bind_host() -> &'static str {
    "127.0.0.1"
}
EOF

cat > "$repo/docs/desktop.md" <<'EOF'
# Desktop

The Electron shell starts `coppice-server desktop` and opens the board.
EOF

git -C "$repo" add .
git -C "$repo" commit -m "Initial desktop bootstrap" >/dev/null

git -C "$repo" worktree add -b "$branch" "$wt" >/dev/null

cat > "$wt/desktop/src/postgres.ts" <<'EOF'
/** Desktop database bootstrap.
 *  Postgres 16 ships inside the app and listens on loopback only.
 */
export function postgresPort(): number {
  return 5432;
}

export function dataDir(home: string): string {
  return `${home}/coppice/postgres`;
}

export function initArgs(dir: string): string[] {
  return [
    "initdb",
    "-D",
    dir,
    "--username",
    "coppice",
    "--auth",
    "trust",
    "--no-sync",
  ];
}

export function startArgs(dir: string, port: number): string[] {
  return ["pg_ctl", "-D", dir, "-o", `-h 127.0.0.1 -p ${port}`, "start"];
}
EOF

cat > "$wt/server/src/desktop_bootstrap.rs" <<'EOF'
/// App data directory used by `coppice-server desktop`.
pub fn data_dir() -> &'static str {
    "coppice-data"
}

pub fn bind_host() -> &'static str {
    "127.0.0.1"
}

/// Refuse a cluster whose major version is not the bundled Postgres 16.
pub fn require_bundled_major(major: u32) -> Result<(), &'static str> {
    if major == 16 {
        Ok(())
    } else {
        Err("data directory was created by a different Postgres major version")
    }
}
EOF

cat > "$wt/docs/desktop-postgres.md" <<'EOF'
# Bundled Postgres

First launch runs `initdb` into the app data directory. Later launches reuse
that cluster, bind `127.0.0.1`, and clear a stale postmaster lock if the
previous process was killed.
EOF

git -C "$wt" add .
git -C "$wt" commit -m "Bundle Postgres 16 with the desktop app" >/dev/null

printf '%s\n' "$wt"
