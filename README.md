[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文（台灣）](README.zh-TW.md) · [繁體中文（香港）](README.zh-HK.md) · [日本語](README.ja.md) · [Français](README.fr.md)

# Coppice

**Beta.** Hire specialists. Keep the final say.

<p align="center">
  <img src="static/marketing.gif" alt="Write a ticket, approve the plan, and review the diff. Nothing is done until you accept it." width="960" />
</p>

**Stop tab-hopping between agent terminals.** Write a ticket and your PM agent drafts a plan for you to approve. Then Claude Code, Codex and the rest build, review and test in parallel, each on its own branch. The board shows what's moving, what's stuck and what's waiting on you, and nothing counts as done until you've reviewed the diff. It runs on your machine with the agent CLIs you already pay for: no new subscription, no Coppice account.

## The gates

Work stops for you in two places:

1. **Plan Review.** Your PM agent drafts a plan, and you approve it or ask for changes before work starts. Planning never changes your ticket's code. Turn on Skip planning to send a ticket straight to Ready.
2. **Wait for Human Review.** You review the diff. The ticket stays in this column until you accept it.

You start the work. Create a ticket, or approve one an agent proposed. Agents plan and build between those gates.

## This beta

Installers for the beta are a `.dmg` for macOS (Apple silicon or Intel) and a `.deb` for Linux (`amd64` or `arm64`). Get them from the [releases page](https://github.com/fertile-org/coppice/releases).

Bring the CLIs you already use — Claude Code, Codex, Cursor, OpenCode and Kilo Code. Install and sign in to them the usual way. Coppice finds them on your login shell `PATH`.

## Get started

Product docs are the site in [`website/`](website/): the landing page and the user guides. The public site is [https://getcoppice.app/](https://getcoppice.app/).

## Contributors

Docker Compose is how contributors and CI run the stack. Start with [AGENTS.md](AGENTS.md) and [docs/development.md](docs/development.md). The roadmap is [docs/milestones/README.md](docs/milestones/README.md).

## License

Coppice is open source under the [Apache 2.0 license](LICENSE).
