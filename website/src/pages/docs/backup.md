---
layout: ../../layouts/Docs.astro
title: Backup data
description: "Back up Coppice, move it to a new computer, and replace keys if a backup leaks."
---

# Backup data

## Download a backup

Open **Tools**, then **Backup**, and choose **Download backup**. You get one file with your boards, tickets, settings, files, and agent worktrees.

Passwords, keys, and saved tokens are left out. The backup still holds all your workspace data, so keep it private.

## Import a backup

On the install you're moving to, open **Tools**, then **Backup**, and choose **Import backup**.

Importing replaces all data on that install with the backup's. Your board and workspace settings come across. Passwords, keys, folder paths, agent commands, and plugin and MCP settings don't, so set those up again. You may be signed out.

Saved tokens aren't in the backup, so paste each one again afterward. Coppice marks each token it needs with "Paste this token again." Paste it, then save.

The desktop app has everything it needs. A server install needs PostgreSQL's `pg_dump` to download a backup and `psql` to import one.

## Copy the folders by hand

You can also quit Coppice and copy its data folder:

| | Data | Logs |
| --- | --- | --- |
| macOS | `~/Library/Application Support/Coppice` | `~/Library/Logs/Coppice` |
| Linux | `~/.config/Coppice` | `~/.local/state/Coppice/logs` |

This copy includes the `secrets` folder, so treat it like a password. Your CLI sign-ins aren't in it. Each CLI keeps its own.

## Replace keys

If you shared a backup, or think someone else has a copy, make new keys.

In the desktop app, open **Tools**, then **Backup**, and choose **Replace keys**. Enter a new admin password and check the box. Coppice makes new keys and a new database password. Your saved tokens keep working, and everyone is signed out.

On a server, stop Coppice, leave PostgreSQL running, and run:

    coppice-server replace-keys

It asks for the new admin password twice, then tells you which file holds the new values. Keep that file private.

## Backups from rc.7 or earlier

Those backups contain your Coppice keys and passwords in plain text. Delete them, or keep them somewhere private. If you shared one, replace your keys.
