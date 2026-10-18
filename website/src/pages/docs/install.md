---
layout: ../../layouts/Docs.astro
title: Install
description: "Coppice Beta ships as a desktop app. No Docker, no server, no Coppice account."
---

# Install

Coppice Beta ships as a desktop app. No Docker, no server, no Coppice account.

## Supported platforms
| Platform | Package |
| --- | --- |
| macOS (Apple Silicon) | `.dmg` |
| Linux (x64) | `.deb` |

There's no Windows app in this beta. On Windows 11, see [Windows 11 (through WSL)](#windows-11-through-wsl).

## Download
- [macOS (Apple Silicon) `.dmg`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-arm64.dmg)
- [macOS (Intel) `.dmg`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-x64.dmg)
- [Linux (x64) `.deb`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-amd64.deb)
- [Linux (ARM64) `.deb`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-arm64.deb)

You can also get every build from [GitHub Releases](https://github.com/fertile-org/coppice/releases).

## Updates

Coppice checks for new versions in the background. To turn this off, open **Settings → Updates** and turn off **Automatic updates**. Your boards and settings stay when you update.

- **macOS:** choose **Download update** in the banner, then drag the new Coppice into Applications.
- **Linux:** installing the `.deb` adds Coppice's package source, so new versions arrive with your other updates in Software Updater, or run `sudo apt update && sudo apt upgrade`.

## Windows 11 (through WSL)
The Linux x64 build should run on Windows 11 through WSL. We haven't tested this yet. If you try it, please [tell us how it went](https://github.com/fertile-org/coppice/issues).

You need Windows 11, because older Windows 10 builds can't show Linux app windows. Windows on Arm PCs aren't covered for now.

1. Open PowerShell as administrator and run `wsl --install`. This installs Ubuntu. Restart when asked, then finish setting up your Ubuntu user.
2. In PowerShell as administrator, update WSL so Linux app windows show on your Windows desktop:

   ```powershell
   wsl --update
   wsl --shutdown
   ```

3. Open Ubuntu from the Start menu. Do the rest of these steps in Ubuntu. Download the Linux x64 `.deb` and install it. Replace `<version>` with the latest version on [GitHub Releases](https://github.com/fertile-org/coppice/releases):

   ```sh
   cd ~
   wget https://github.com/fertile-org/coppice/releases/download/v<version>/Coppice-<version>-linux-amd64.deb
   sudo apt install ./Coppice-<version>-linux-amd64.deb
   ```

   Use `apt`, not `dpkg -i`. `apt` also installs what Coppice needs.

   To update Coppice later, run `sudo apt update && sudo apt upgrade`. Your boards and settings stay.
4. Install the agent CLIs you use (Claude Code, Codex, Cursor, OpenCode, Kilo Code) in Ubuntu. Sign in to them and to git there too.
5. Start Coppice from the Windows Start menu (under Ubuntu), or run `coppice` in Ubuntu.

   If a CLI shows Not on your PATH, start Coppice by running `coppice` in Ubuntu.

WSL adds your Windows PATH to Ubuntu, so Coppice could find a Windows copy of a CLI. Open **System → Connectors** and look at each connector's CLI path. If a path starts with `/mnt/c/`, install that CLI in Ubuntu.

Keep your repos in your Linux home folder, for example `~/code/my-app`, not under `/mnt/c/`. Git and file access are much slower across that boundary. The folder picker shows Linux paths.

## First run
1. Install and open Coppice.
2. Point it at a local git checkout you already have on disk.
3. **Connect your agent CLI.** Pick your agent CLI when you create an agent, and Coppice turns it on. Each one shows whether it's **Ready**, **Found, not signed in**, or **Not on your PATH**. Sign in to the CLI in your terminal first, the same way you normally would.

   Kilo Code shows **Kilo Code — Installed**. System → Connectors adds: "Coppice can't check this sign-in without using your plan. A run will show an error if you're signed out."

   To turn a connector off later, use the toggle in System → Connectors, or set `enabled = false` for it in your Coppice `config.toml`:

   - macOS: `~/Library/Application Support/Coppice/config.toml`
   - Linux: `~/.config/Coppice/config.toml`

4. Create a ticket (or approve a proposed one) and move it to Ready. Agents take it through In Progress, In Review and In QA, then it waits for your Human Review.

## Requirements
- A local git repository you can write to (Coppice uses isolated worktrees)
- At least one supported agent CLI installed and authenticated the normal way for that tool

## Troubleshooting
If the app won’t start or an agent never picks up a ticket, check the live terminal on the ticket and the FAQ. For build-from-source, see the [contributor docs](https://github.com/fertile-org/coppice/blob/main/docs/development.md) in the repo.
