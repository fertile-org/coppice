---
name: design-system
description: Use when a ticket asks for a DESIGN.md. Read the project's existing tokens and screens, then write one DESIGN.md at the repo root.
---

<!-- Adapted from google-labs-code/design.md@db977b3c1d1f309269f43d5d17ae828d54bd6ae3 (Apache-2.0). See THIRD_PARTY_NOTICES.md. -->

Write `DESIGN.md` at the repo root. It is the project's design source for later UI work. Do this on a work ticket, in the ticket worktree. A planning run's scratch folder is discarded, so a plan only proposes this ticket; it does not leave the file behind.

Read first, in this order:

1. Any `DESIGN.md` already in the repo. Update it. Do not replace facts you did not check.
2. Theme and token files: CSS variables, `tokens.json`, or a Tailwind theme.
3. Two or three existing screens. Name the components people already see.

The file has two parts:

- Short markdown that says what the product looks like and when to use each token.
- YAML design tokens. Put them in front matter between `---` lines, or in fenced `yaml` blocks under the matching section. Colors, typography, and spacing are enough. Each top-level key appears once. Token values are the source of truth. Prose may use friendly color names, and those names must match a token.

Use the colors, type, and spacing already in the code. Do not invent a second palette. If the repo has almost no UI yet, write a small starter from what the ticket describes, and say which values are new.

Keep the file short enough to read on one screen. Do not copy a vendor spec into it.
