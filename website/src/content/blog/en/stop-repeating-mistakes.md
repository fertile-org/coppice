---
title: How to stop your AI coding agent repeating the same mistakes
metaTitle: Stop an AI agent repeating mistakes
description: "Put the correction in a rules file, a failing check, the task, or a skill. In Coppice, approve a Knowledge note so a later matching ticket lists the note."
answer: "Write the correction where the next session will see it. A rules file such as CLAUDE.md, AGENTS.md, or a Cursor rule is loaded into later sessions. A test or lint check fails when the mistake returns. You can also put it in the task, or in a skill the agent loads when the task fits. In Coppice, save it as a Knowledge note and approve it. On a later ticket whose words match, the agent is told to read that note."
published: "2026-11-11"
draft: false
author: Coppice
authorType: Organization
keywords:
  - AI coding agents
  - CLAUDE.md
  - AGENTS.md
  - Cursor rules
  - Knowledge
---

## Why this is hard

You correct the agent. The next task starts clean, and the mistake shows up again. A coding agent does not keep that correction once the chat ends. Something has to put it in front of the next run, or a check has to fail when the mistake comes back.

## Option 1: A rules file in the project

Claude Code reads `CLAUDE.md` at the start of every session, from the folder you start in and the folders above it. When that path has no `CLAUDE.md` and no `CLAUDE.local.md`, it reads `AGENTS.md` instead. It also loads the first 200 lines of its auto memory file, `MEMORY.md`, or 25KB, whichever comes first. Claude Code treats these files as context. They do not block an edit.

Codex reads `AGENTS.md` before it works. It walks from the project root down to the folder you started in, and stops once the combined files reach 32 KiB.

Cursor reads `AGENTS.md` in the project root and in subfolders of the files you are working on. A project rule in `.cursor/rules` is in every chat when you set it to always apply. Otherwise Cursor includes it when the files match, when the agent decides it fits, or when you mention it. A rules file is advice. The agent can skip a line.

## Option 2: A test or a lint check

When a check can see the mistake, add the check. A test that fails when a Settings control keeps its own theme state, or a lint rule for that pattern, stops the change on the next run.

This blocks the mistake. A layout choice still needs a person, because a check cannot see it.

## Option 3: Put it in the task, or in a skill

Paste the correction into this task. The agent sees it for this run. The next task needs the paste again.

A skill is a short instructions file the agent loads when the task fits, or when you invoke it. In Claude Code the file stays out of the session until then, which is why a one-line rule belongs in `CLAUDE.md` and a procedure belongs in a skill. The agent can still skip it.

## Option 4: A Knowledge note you approve, with Coppice

[Coppice](/) is a desktop app that puts your coding agents on a task board. Knowledge is a note about the project for later tickets. You write the note, or during a ticket the agent can propose one for a rule a later ticket would need. It stays pending until you approve it. Later runs cannot see a pending note.

A note can cover every board, one board, or one agent on a board. A note the agent proposes from a ticket covers that board.

1. [Install Coppice](/docs/install) and add your project.
2. Open Knowledge and add a note. For a dark mode toggle in Settings, write that the control uses the existing theme hook, and that the change includes a settings test. Choose the board the work is on.
3. Approve the note.
4. On the next ticket, click **Run agent**. Coppice searches approved notes for words in the ticket title and description. A match is listed for the agent at the start of the run, and the agent is told to read it before relying on it.

A later ticket titled "Add a dark mode toggle to Settings" shares those words, so the note is listed. The full text stays out of the prompt until the agent reads it. The agent can also search approved notes. A terminal outside Coppice never sees the note. [Concepts](/docs/concepts#knowledge) describes Knowledge.

![Add knowledge dialog. The title is Settings theme controls use the existing theme hook, the type is Review feedback, the board is acme-web, and the note says a dark mode toggle uses the existing theme hook and includes a settings test.](/blog/stop-repeating-mistakes/add-note.png "The note names the theme hook, on the acme-web board.")

![A pending Knowledge note on the acme-web board. The title is Settings theme controls use the existing theme hook, and Approve is on the card.](/blog/stop-repeating-mistakes/pending-note.png "The note stays pending until you approve it.")

## Which one to pick

- **The mistake can fail a check:** add a test or a lint rule.
- **Every session in one tool should see a short rule:** `CLAUDE.md`, `AGENTS.md`, or a Cursor rule.
- **One task, or a procedure you do not want in every session:** write it in the task, or put it in a skill.
- **Many tickets, and you want to approve what is kept before the next run can use it:** Coppice. Start with [Getting started](/docs/getting-started).
