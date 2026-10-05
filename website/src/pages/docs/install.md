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
- [macOS arm64 `.dmg`](#) — replace with GitHub Release asset when tagged
- [Linux x64 `.deb`](#) — replace with GitHub Release asset when tagged

Or grab the latest assets from [GitHub Releases](https://github.com/fertile-org/coppice/releases).

## First run
1. Install and open Coppice.
2. Point it at a local git checkout you already have on disk.
3. Connect an agent CLI you already use (see [Providers](/docs/providers)).
4. Create a ticket (or approve a proposed one), move it to Ready, and walk Plan Review → execution → Final Review.

## Requirements
- A local git repository you can write to (Coppice uses isolated worktrees)
- At least one supported agent CLI installed and authenticated the normal way for that tool

## Troubleshooting
If the app won’t start or an agent never picks up a ticket, check the live terminal on the ticket and the FAQ. For build-from-source, see the [contributor docs](https://github.com/fertile-org/coppice/blob/main/docs/development.md) in the repo.
