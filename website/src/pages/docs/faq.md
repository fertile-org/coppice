---
layout: ../../layouts/Docs.astro
title: FAQ
metaTitle: "Coppice FAQ: Beta, Docker, Windows and Merging"
description: "Is Coppice production-ready? Do you need Docker or an account? Will agents merge without you? Answers about the Beta, Windows, conflicts and more."
---

# FAQ

## Is Coppice production-ready?
No, not yet. Coppice is in beta, for early users who want a local agent board with real human gates. Agents run with your local permissions.

## Do I need Docker or an account?
No. Download the desktop app, open it, and work. No Docker, no server setup, no Coppice account.

## Is this a Jira / Linear replacement?
No. Coppice is a board for driving coding agents, not a full company issue tracker.

## Is this a cloud coding agent?
No. Agents run via CLIs on your machine, on your repos. A cloud agent service hands the work to someone else's computer. Coppice keeps the work on your machine. You start the work, and you accept every change.

## Will agents merge without me?
No. Human Review is the gate. You open the diff and decide.

## What happens if a ticket's changes conflict with main?
Coppice stops before merging and leaves main untouched. It lists the files that conflict. The ticket stays in Done until you choose **Ask [agent] to resolve**, which sends it back to In Progress. You then accept it again in Human Review before it merges.

## What about “set and forget” autopilots?
That is not Coppice's product. Coppice is for managing agents: you start the work, and you accept every change.

## Windows?
Windows 11 should be able to run the Linux x64 build through WSL, but we haven't tested it yet. See [Windows 11 (through WSL)](/docs/install#windows-11-through-wsl). If you try it, please [tell us how it went on GitHub](https://github.com/fertile-org/coppice/issues). A native Windows app isn't planned for this beta.

## Where do contributors look?
Architecture, development, and testing docs stay in the [GitHub repo](https://github.com/fertile-org/coppice/tree/main/docs). This site is for using Coppice.
