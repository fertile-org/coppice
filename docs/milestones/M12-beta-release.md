# M12 — Beta Release

## Goal

Ship a public **Beta**: a marketing and user-docs site plus an installable desktop app. Platforms for this beta are **macOS Apple Silicon** (`.dmg`) and **Linux x64** (`.deb`) only. No Windows installer. The site, the docs, and the root README say Beta, and they use the locked positioning:

> Agents plan, you approve. Agents build, you review. On your machine, with the agent CLIs you already pay for.

Human gates are **Plan Review** and **Wait for Human Review**. The beta is that manager workflow on a local board. It is not a set-and-forget autopilot.

Ship this before the capability sandbox ([M13 — Security & sandbox](./M13-security-and-sandbox.md)) and before role-owner agents ([M14 — Role-owner agents](./M14-role-owner-agents.md)). Do not sell role-owners as the beta hero. If the site mentions them, they are a later milestone.

## Product scope

- **Astro site** at `website/`, in the spirit of a small product site (landing page plus user docs). Web docs are the primary guide for people using Coppice. Repo `docs/` stays the contributor set (architecture, development, milestones) and stays slimmer than the site.
- **Root README** leads with the Electron desktop app. Docker Compose remains the contributor and CI stack, documented from [AGENTS.md](../../AGENTS.md) and [development.md](../development.md).
- **Platforms:** macOS arm64 `.dmg` and Linux x64 `.deb`. Copy does not offer Windows, macOS Intel, or Linux arm64 as beta downloads.
- **Beta labeling** on the site (including docs pages) and in the root README.
- **Download CTAs** resolve to GitHub Release assets only after a beta git tag has published those two installers. Until that release exists, the buttons do not pretend a file is there.
- **No mock agent in end-user builds.** `MockProvider` stays for automated tests and for Compose/dev when the `mock-provider` feature is on. Desktop installers and `make release-tar` are built without that feature ([mock](../providers/mock.md)).
- **Screenshot** in the root README stays `static/screenshot.png` (the board). Surrounding copy does not describe Sign out or an `admin@localhost` login.

## Out of scope

- Capability sandbox, tool policy, scoped secret injection, guided unblock, audit log (M13)
- Role-owner observation runs, workspace signals, Workspace Inbox (M14)
- A custom domain. [https://getcoppice.vercel.app/](https://getcoppice.vercel.app/) is the public site until one is chosen
- Choosing or pushing the beta git tag as part of the docs-only renumber. The tag is a release action
- Windows, auto-update, or in-app CLI install

## Blockers carried from M11

[M11 — Desktop release](./M11-desktop-release.md) is implemented, and these leftovers still block calling the beta shipped:

- **Live tag check.** Pushing a version tag produces a draft GitHub Release whose installers and matrix smokes are green.
- **Manual install with a real connector.** On a clean Mac, install the arm64 `.dmg`. On a fresh Ubuntu x64 machine, install the `.deb`. The app opens to the board with no login screen; a registered repo and a **real** connector take a ticket to Done. A mock-to-Done run is not a valid check: MockProvider is not in the installer.

M10's live connector verification (`claude-code`, `codex`, `kilo-code`) stays open on its own milestone. It does not replace the real-connector install check above.

## Acceptance criteria

These are checkable with the repo, GitHub Releases, and GitHub Pages. They do not require a custom domain or any host Hung has not already chosen.

- [ ] `website/` is an Astro site with a landing page and user docs (install, concepts, and providers at minimum). `npm run build` in `website/` succeeds.
- [ ] Landing and docs pages say **Beta**.
- [ ] Landing and docs use the positioning one-liner above and name **Plan Review** and **Wait for Human Review**. Role-owner agents are not the hero; any mention points at M14.
- [ ] The only download targets named for the beta are macOS Apple Silicon `.dmg` and Linux x64 `.deb`.
- [ ] Before a beta git tag's GitHub Release contains those assets, download CTAs do not link to a missing file. After that release exists, each CTA returns the matching asset (HTTP 200, expected filename).
- [ ] [https://getcoppice.vercel.app/](https://getcoppice.vercel.app/) serves the built site at the domain root. A custom domain is not required. The website workflow can still deploy GitHub Pages at `/coppice` when that dispatch is run.
- [ ] Root [README.md](../../README.md) leads with the Electron desktop app, points at `website/` and `https://getcoppice.vercel.app/` as the product docs, and points contributors at [AGENTS.md](../../AGENTS.md) and [docs/development.md](../development.md). Docker Compose is the contributor/dev path.
- [ ] The README image is `static/screenshot.png`. The README text does not tell the reader to sign in as `admin@localhost` or to look for a Sign out control on that shot.
- [ ] A desktop installer and a `make release-tar` build do not list or run the `mock` connector. Automated tests, and Compose/dev with `--features mock-provider`, still can ([mock](../providers/mock.md)).
- [ ] The M11 live tag check has passed, and manual install acceptance on both the arm64 `.dmg` and the x64 `.deb` used a real connector through to Done.

## References

- [M11 — Desktop release](./M11-desktop-release.md)
- [M13 — Security & sandbox](./M13-security-and-sandbox.md)
- [M14 — Role-owner agents](./M14-role-owner-agents.md)
- [Mock provider](../providers/mock.md)
