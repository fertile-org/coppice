#!/bin/sh
# Demo checkout for the article stills. Args: init | worktree <ticket-short-id>
# The repository name on screen is Demo. This is not a project checkout.
set -eu

repo=/tmp/coppice-screenshot-repo
mode="${1:-init}"

if [ "$mode" = "init" ]; then
  rm -rf "$repo"
  mkdir -p "$repo/src"
  git init -b main "$repo" >/dev/null
  git -C "$repo" config user.email screenshot@coppice.local
  git -C "$repo" config user.name "Coppice Screenshot"
  printf '%s\n' '# Demo' > "$repo/README.md"
  printf '%s\n' 'export function note() { return "demo"; }' > "$repo/src/note.js"
  git -C "$repo" add .
  git -C "$repo" commit -m "Start the demo checkout" >/dev/null
  exit 0
fi

if [ "$mode" != "worktree" ]; then
  printf '%s\n' "usage: seed-demo-repo.sh init | worktree <ticket-short-id>" >&2
  exit 1
fi

short="$2"
wt="/data/worktrees/TICKET-${short}-demo"
branch="agent/TICKET-${short}"
rm -rf "$wt"
git -C "$repo" worktree add -b "$branch" "$wt" >/dev/null
printf '%s\n' 'export function note() { return "reviewed"; }' > "$wt/src/note.js"
git -C "$wt" add src/note.js
git -C "$wt" commit -m "Update the demo note" >/dev/null
printf '%s\n' "$wt"
