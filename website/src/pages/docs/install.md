---
layout: ../../layouts/Docs.astro
title: Install
description: "Coppice Beta ships as a desktop app. No Docker, no server, no account."
---

# Install

Coppice Beta ships as a desktop app. No Docker, no server, no account.

## Supported platforms
| Platform | Package |
| --- | --- |
| macOS (Apple Silicon) | `.dmg` |
| Linux (x64) | `.deb` |

Windows is not available in this Beta.

## Download
- [macOS arm64 `.dmg`](#) — Coming with the beta release.
- [Linux x64 `.deb`](#) — Coming with the beta release.

After the beta tag, assets will also appear on [GitHub Releases](https://github.com/fertile-org/coppice/releases).

## First run
1. Install and open Coppice.
2. Point it at a local git checkout you already have on disk.
3. **Connect your agent CLI.** Pick your agent CLI when you create an agent, and Coppice turns it on. Each one shows whether it's **Ready**, **Found, not signed in**, or **Not on your PATH**. Sign in to the CLI in your terminal first, the same way you normally would.

   Coppice can't check Kilo Code's sign-in, so it shows no status. Make sure you're signed in to it in your terminal.

   To turn a connector off later, use the toggle in Tools → Connectors, or set `enabled = false` for it in your Coppice `config.toml`:

   - macOS: `~/Library/Application Support/Coppice/config.toml`
   - Linux: `~/.config/Coppice/config.toml`

4. Create a ticket (or approve a proposed one) and move it to Ready. Agents take it through In Progress, In Review and In QA, then it waits for your Final Review.

## Requirements
- A local git repository you can write to (Coppice uses isolated worktrees)
- At least one supported agent CLI installed and authenticated the normal way for that tool

## Troubleshooting
If the app won’t start or an agent never picks up a ticket, check the live terminal on the ticket and the FAQ. For build-from-source, see the [contributor docs](https://github.com/fertile-org/coppice/blob/main/docs/development.md) in the repo.
