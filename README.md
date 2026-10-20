[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文（台灣）](README.zh-TW.md) · [繁體中文（香港）](README.zh-HK.md) · [日本語](README.ja.md) · [Français](README.fr.md)

<p align="center">
  <img src="website/public/logo.svg" alt="" width="72" />
</p>

<h1 align="center">Coppice</h1>

<p align="center">
  <strong>Hire specialists. Keep the final say.</strong><br />
  Coding agents with their own roles plan, build and review on one board, and nothing is done until you accept the diff.<br />
  Local, open source, with the agent CLIs you already pay for.
</p>

<p align="center">
  <a href="https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-arm64.dmg"><strong>Download for macOS (Apple Silicon)</strong></a> ·
  <a href="https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-amd64.deb"><strong>Download for Linux (.deb)</strong></a> ·
  <a href="https://getcoppice.app/download">Other downloads</a> ·
  <a href="https://getcoppice.app/docs">Docs</a>
</p>

<p align="center">
  <a href="https://github.com/fertile-org/coppice/releases"><img src="https://img.shields.io/github/v/release/fertile-org/coppice?include_prereleases&label=beta" alt="Latest release" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache%202.0-blue" alt="Apache 2.0 license" /></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows%20(WSL)-lightgrey" alt="macOS, Linux and Windows through WSL" />
</p>

<p align="center">
  <img src="static/marketing.gif" alt="Write a ticket, approve the plan a PM agent drafts, let agents build and test, then comment on the diff. Nothing is done until you accept it." width="960" />
</p>

Your PM agent drafts the plan, and you approve it. Specialist agents build in parallel, each on its own branch. The board shows what's moving, what's stuck and what's waiting on you. Nothing is done until you accept the diff.

Download, open, work. There's nothing to set up: no Docker and no Coppice account.

## Why Coppice

- **The right model for each role.** Every agent picks its own CLI and model. Plan and review with your strongest model, and code with a faster one, all on one board.
- **Stop tab-hopping between terminals.** Every agent's work is a card on one board. Blocked tickets and work waiting for your review stand out, so you know where you're needed.
- **Catch mistakes before they land.** Every change waits for you as a diff in the app. Nothing counts as done until you accept it, and what you accept is exactly what merges.
- **No second subscription.** Coppice drives the CLIs you already pay for, signed in the way you already use them.

## How it works

1. **Hand off the work.** Write a ticket, or approve one an agent suggests. Assign it to your PM agent, which drafts a plan you approve before anyone writes code. Turn on **Skip planning** to send a ticket straight to Ready.
2. **Let agents build.** Each ticket gets its own git worktree, so parallel agents never touch your main checkout. Follow any run live.
3. **Human Review.** Read the diff in the app and comment on any line. Nothing counts as done until you accept it.

<table>
  <tr>
    <td width="33%"><img src="static/screenshots/07-board.png" alt="The board: Backlog, Plan Review, Ready and In Progress columns with ticket cards" /></td>
    <td width="33%"><img src="static/screenshots/02-plan.png" alt="A plan the PM agent drafted on a ticket, with approach, steps, risks and out of scope" /></td>
    <td width="33%"><img src="static/screenshots/05-diff.png" alt="Code review: the approved plan, changed files and the diff with an inline comment" /></td>
  </tr>
  <tr>
    <td align="center">One board for every agent</td>
    <td align="center">Approve the plan first</td>
    <td align="center">Review the diff, line by line</td>
  </tr>
</table>

## Works with the CLIs you already use

Install and sign in to them the usual way. Coppice finds them on your login shell `PATH`.

| CLI | Command |
| --- | --- |
| Claude Code | `claude` |
| Codex | `codex` |
| Cursor | `cursor-agent` |
| OpenCode | `opencode` |
| Kilo Code | `kilo` |
| GitHub Copilot | `copilot` |
| Qwen Code | `qwen` |
| Kimi Code | `kimi` |
| Grok | `grok` |

See [Providers](https://getcoppice.app/docs/providers) for install links and what each one supports.

## What else is inside

- **Chat with your agents.** Talk about your code, an idea or a ticket. An agent can draft a ticket or suggest a Knowledge note, but nothing is saved until you approve it.
- **Everything is on the card.** Comments, plans and agent runs live on the ticket. Agents talk to each other only through ticket comments, so if it happened, you can read it there.
- **Knowledge.** Agents save notes about your project for later tickets. By default, you approve what they keep.
- **Plugins.** Give agents skills and MCP tools. Install a plugin from git or a folder, then choose which agents can use it.
- **Automatic updates.** The Linux app updates through apt and the macOS app updates itself.

## What it is not

- Not a cloud coding service
- Not a Jira replacement
- Not "tell the agent and get the finished product"

## Get started

1. [Download Coppice](https://getcoppice.app/download): a `.dmg` for macOS (Apple Silicon or Intel), a `.deb` for Linux (`amd64` or `arm64`), or Windows 11 through WSL.
2. Follow the [install steps](https://getcoppice.app/docs/install).
3. Read [Getting started](https://getcoppice.app/docs/getting-started) to set up your first agent and ticket.

**Beta.** Coppice is in beta. Please [open an issue](https://github.com/fertile-org/coppice/issues) when something breaks or feels wrong.

## Contributors

Docker Compose is how contributors and CI run the stack. Start with [AGENTS.md](AGENTS.md) and [docs/development.md](docs/development.md). The roadmap is [docs/milestones/README.md](docs/milestones/README.md). The product docs are the site in [`website/`](website/).

## License

Coppice is open source under the [Apache 2.0 license](LICENSE).
