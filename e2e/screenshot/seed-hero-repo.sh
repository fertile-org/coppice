#!/bin/sh
# Git checkout for the homepage hero. Args: init | worktree <ticket-short-id>
# The repo name on screen is acme-web. The diff adds a settings toggle that
# keeps its own state, so a review comment can point at the theme hook.
set -eu

repo=/tmp/coppice-screenshot-repo
mode="${1:-init}"

if [ "$mode" = "init" ]; then
  rm -rf "$repo"
  mkdir -p "$repo/src/settings" "$repo/src/theme"
  git init -b main "$repo" >/dev/null
  git -C "$repo" config user.email hero@coppice.local
  git -C "$repo" config user.name "Coppice Hero"

  cat > "$repo/README.md" <<'EOF'
# acme-web

The marketing site and the signed-in app share one settings page.
EOF

  cat > "$repo/src/theme/useTheme.ts" <<'EOF'
export type ThemeName = 'light' | 'dark';

export function useTheme(): {
  theme: ThemeName;
  setTheme: (next: ThemeName) => void;
} {
  return {
    theme: 'light',
    setTheme: () => {},
  };
}
EOF

  cat > "$repo/src/settings/ThemeToggle.tsx" <<'EOF'
export function SettingsHeading() {
  return <h1>Settings</h1>;
}
EOF

  git -C "$repo" add .
  git -C "$repo" commit -m "Describe settings for acme-web" >/dev/null
  exit 0
fi

if [ "$mode" != "worktree" ]; then
  echo "usage: seed-hero-repo.sh init | worktree <ticket-short-id>" >&2
  exit 1
fi

short="$2"
slug="acme-web"
wt="/data/worktrees/TICKET-${short}-${slug}"
branch="agent/TICKET-${short}"

rm -rf "$wt"
git -C "$repo" worktree add -b "$branch" "$wt" >/dev/null

cat > "$wt/src/settings/ThemeToggle.tsx" <<'EOF'
import { useState } from 'react';

export function SettingsHeading() {
  return <h1>Settings</h1>;
}

export function ThemeToggle() {
  const [theme, setTheme] = useState('light');

  return (
    <button
      type="button"
      onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
    >
      {theme === 'dark' ? 'Use light mode' : 'Use dark mode'}
    </button>
  );
}
EOF

git -C "$wt" add .
git -C "$wt" commit -m "Add a dark mode toggle to Settings" >/dev/null
printf '%s\n' "$wt"
