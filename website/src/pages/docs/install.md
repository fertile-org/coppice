---
layout: ../../layouts/Docs.astro
title: Install
metaTitle: "Install Coppice on macOS, Linux and Windows (WSL)"
description: "Install Coppice on macOS (Apple Silicon or Intel) or Linux (x64 or ARM64), or on Windows 11 through WSL. No Docker and no Coppice account."
---

# Install

Coppice Beta is a desktop app. You don't need Docker, a server, or a Coppice account. You do need a local git checkout and one supported agent CLI, signed in with that CLI. [Providers](/docs/providers) lists the CLIs. Coppice does not install them, and it never asks for a provider API key.

## Packages by platform and architecture

| Platform | Architecture | Package | Setup |
| --- | --- | --- | --- |
| macOS | Apple Silicon (arm64) | `.dmg` | Open the disk image and move Coppice to Applications. Then follow [Getting started](/docs/getting-started). |
| macOS | Intel (x64) | `.dmg` | Same as Apple Silicon. |
| Linux | x64 (amd64) | `.deb` | [Install the package](#linux). |
| Linux | ARM64 | `.deb` | [Install the package](#linux). |
| Windows 11 | x64, through WSL | No Windows package. Use the Linux x64 `.deb` inside Ubuntu. | [Windows 11 (through WSL)](#windows-11-through-wsl). We haven't tested this path. Windows on Arm isn't covered. |

Not sure which Mac you have? Open Apple menu → About This Mac. If it lists a Chip, choose Apple Silicon. If it lists a Processor, choose Intel.

## Download
- [macOS (Apple Silicon) `.dmg`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-arm64.dmg)
- [macOS (Intel) `.dmg`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-x64.dmg)
- [Linux (x64) `.deb`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-amd64.deb)
- [Linux (ARM64) `.deb`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-arm64.deb)

You can also get every build from [GitHub Releases](https://github.com/fertile-org/coppice/releases).

## Linux

Download the `.deb` for your architecture from the list above. In the folder that contains the file, install the x64 package with `sudo apt install ./Coppice-linux-amd64.deb`. On ARM64, use `./Coppice-linux-arm64.deb` instead. Use `apt`, not `dpkg -i`. `apt` also installs what Coppice needs.

Then follow [Getting started](/docs/getting-started).

## Updates

Coppice checks for new versions in the background. To turn this off, open **System → Updates** and turn off **Automatic updates**. Your boards and settings stay when you update.

- **macOS:** Coppice downloads the update for you. Choose **Restart to update** in the banner, or the update installs the next time you quit Coppice.
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
4. Install the agent CLIs you use (Claude Code, Codex, Cursor, OpenCode, Kilo Code, GitHub Copilot, Qwen Code, Kimi Code, Grok) in Ubuntu. Sign in to them and to git there too.
5. Start Coppice from the Windows Start menu (under Ubuntu), or run `coppice` in Ubuntu.

   If a CLI shows **Not on your PATH**, start Coppice by running `coppice` in Ubuntu.

WSL adds your Windows PATH to Ubuntu, so Coppice could find a Windows copy of a CLI. Open **System → Connectors** and look at each connector's CLI path. If a path starts with `/mnt/c/`, install that CLI in Ubuntu.

Keep your repos in your Linux home folder, for example `~/code/my-app`, not under `/mnt/c/`. Git and file access are much slower across that boundary. The folder picker shows Linux paths.

## First run
1. Install and open Coppice.
2. Point it at a local git checkout you already have on disk.
3. Sign in to your agent CLI in your terminal before a ticket runs. Coppice does not ask for a provider API key. [Getting started](/docs/getting-started) walks through the connector check. [Providers](/docs/providers) lists the CLIs and the statuses you can see.

   To turn a connector off later, use the toggle in System → Connectors, or set `enabled = false` for it in your Coppice `config.toml`:

   - macOS: `~/Library/Application Support/Coppice/config.toml`
   - Linux: `~/.config/Coppice/config.toml`

4. Create a ticket and run it. You approve the plan before coding starts, unless you choose Skip planning. Once you start a ticket, agents can continue automatically through the workflow. You review and accept the final diff. Accept and Merge are separate choices.

## Requirements
- A local git repository you can write to (Coppice uses isolated worktrees)
- At least one supported agent CLI installed and signed in the way that CLI normally signs in. See [Providers](/docs/providers).

## Troubleshooting
If the app won't start, or an agent never picks up a ticket, check the live terminal on the ticket and the FAQ. For a build from source, see the [contributor docs](https://github.com/fertile-org/coppice/blob/main/docs/development.md) in the repo.
