---
layout: ../../layouts/Docs.astro
title: Backup data
description: "Back up Coppice, move it to a new computer, and replace keys if a backup leaks."
---

# Backup data

## Download a backup

Open **System**, then **Backup**, and choose **Download backup**. You get one file with your boards, tickets, settings, files, and agent worktrees.

Passwords, keys, and saved tokens are left out. The backup still holds all your workspace data, so keep it private.

## Import a backup

On the computer you're moving to, open **System**, then **Backup**, and choose **Import backup**.

Importing replaces all data on that computer with the backup's. Your board and workspace settings come across. Passwords, keys, folder paths, agent commands, and plugin and MCP settings don't, so set those up again. You may be signed out.

Saved tokens aren't in the backup, so paste each one again afterward. Coppice marks each token it needs with "Paste this token again." Paste it, then save.

## Copy the folders by hand

You can also quit Coppice and copy its data folder:

| | Data | Logs |
| --- | --- | --- |
| macOS | `~/Library/Application Support/Coppice` | `~/Library/Logs/Coppice` |
| Linux | `~/.config/Coppice` | `~/.local/state/Coppice/logs` |

This copy includes the `secrets` folder, so treat it like a password. Your CLI sign-ins aren't in it. Each CLI keeps its own.

## Replace keys

If you shared a backup, or think someone else has a copy, make new keys.

Open **System**, then **Backup**, and choose **Replace keys**. Enter a new admin password and check the box. Coppice will make new keys and database passwords. Saved tokens keep working, and everyone is signed out.
