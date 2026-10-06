---
layout: ../../layouts/Docs.astro
title: FAQ
description: "Beta scope, what’s next, and what Coppice is not."
---

# FAQ

## Is Coppice production-ready?
No, not yet. Coppice is in beta, for early users who want a local agent board with real human gates. Agents run with your local permissions for now, and stronger sandboxing is coming in a later release.

## Do I need Docker or an account?
No. Download the desktop app, open it, and work. No Docker, no server setup, no login.

## Is this a Jira / Linear replacement?
No. Coppice is a board for driving coding agents, not a full company issue tracker.

## Is this a cloud coding agent?
No. Agents run via CLIs on your machine, on your repos. Category contrast: cloud agent services hand off work to someone else’s computer; Coppice keeps the work local and gated by you.

## Will agents merge without me?
No. Final Review is a first-class gate. You open the diff and decide.

## What about “set and forget” autopilots?
That is not Coppice’s product. The wedge is the manager workflow: approve the plan, approve the diff.

## Windows?
Not in this Beta. macOS (Apple Silicon) and Linux (x64) only.

## What’s next on the roadmap?
- Next: security and sandboxing for agents
- Role-owner agents (propose and signal; humans still approve) and scheduling  

## Where do contributors look?
Architecture, development, and testing docs stay in the [GitHub repo](https://github.com/fertile-org/coppice/tree/main/docs). This site is for using Coppice.
