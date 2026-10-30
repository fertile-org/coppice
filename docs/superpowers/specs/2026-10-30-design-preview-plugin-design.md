# Design preview plugin

## Problem

Coppice planning is text plus mermaid. For UI work that is not enough: the human needs to see a clickable mock and pick among options, the way Superpowers drafts a webpage and shows A/B/C.

Coppice drives many CLIs. A live “wait for click” MCP call is not portable (60s tool timeout, 30min run timeout, plan-mode MCP refusals). The pick must survive for the **next** run.

## Requirements

- Human picks among 2–4 options the agent drafted (not a single review, not preview-only).
- Agent **publishes** designs during a run (non-blocking). Human **Choose** is stored. The next planning, chat, or ticket-work turn reads the pick. The live CLI is not paused.
- Cards appear on any agent comment or chat message (planning, chat, ticket work).
- First-party plugin named `design`: auto-enabled, All agents, can be turned off per agent. Ships a skill that tells agents when and how to publish.
- Tools run in the Coppice MCP gateway (`design__…`), not a stdio child. Every connector sees the same tools.
- Coppice owns Choose. The iframe is preview only. Agent HTML cannot steal the pick.
- Files live in a Coppice-owned folder under artifacts, never the git worktree, never `~/.cursor`.
- Preview works in the desktop app and in the browser (SPA iframe, not an Electron-only webview).
- Planning stays read-only on the worktree.

## Design

### Plugin

`design` is a built-in plugin materialized next to `coppice` (extend `materialize_builtin`). It is **enabled** and **All agents** on first start. The Plugins page and the agent form can disable it or drop agents. Only enabled + assigned agents get the tools and the skill.

The skill (`design-ui-options`) says: on UI work, call `design__list` first; if the human has not chosen, draft 2–3 options, `design__create` then `design__write` then `design__publish`; do not put HTML in the ticket body. Next turn, honor `chosen`.

### Tools

Gateway tools, namespaced `design__`. Available on ticket runs and chat turns when the plugin is on for that agent. `readOnlyHint` is false (they write artifacts) but they are allowed on planning and chat because they do not write the worktree.

| Tool | Args | Result |
| --- | --- | --- |
| `design__create` | `title` | `setId`, absolute `dir` under artifacts |
| `design__write` | `setId`, `path` (relative), `content` (utf-8) | `{ path }` written under that set’s dir |
| `design__publish` | `setId`, `options`: 2–4 items `{ id, title, entry }` | `{ setId }` and a visible card on this run’s comment or chat message |
| `design__list` | none (scope = this ticket or chat) | sets newest first: title, options, `chosen` (`null` or `{ id, title }`) |

`entry` is a path relative to that set’s `dir` (must be `*.html`). `id` is a short slug (`a`, `list`). A second publish on the **same run** replaces that run’s card. A later run creates a new set; the old card stays and is marked superseded when a newer published set exists.

Rejected with a short tool error: plugin off, not 2–4 options, missing entry file, path escape (`..` or absolute), folder over 2 MB, disallowed file types (not `html` / `css` / `png` / `jpg` / `jpeg` / `gif` / `webp`). No `svg` (scriptable). Failed publish leaves no card.

There is no Choose tool.

### Storage

Table `design_sets`:

- `id`, `title`
- exactly one of `ticket_id` or `chat_session_id`
- `run_id`, `agent_id`
- `dir` (relative to artifacts)
- `options` jsonb: `[{ id, title, entry }]`
- `chosen_option_id`, `chosen_by_user_id`, `chosen_at`
- `published_at`, `superseded_by` (nullable set id)

Files: `<artifacts>/designs/<set-id>/`. HTML is never written to `context.md`, comments, or prompts.

### Human API

- `POST /api/designs/{setId}/choose` `{ optionId }` — last pick on that set wins. 400 unknown option, 403 if the user cannot comment on that ticket / is not in that chat.
- `GET /api/designs/{setId}` — card payload (titles, chosen, option ids). Session auth.
- File bytes: the SPA fetches with the session, then gives the iframe a **blob URL**. The iframe never loads `/api` and never sees Coppice cookies.

### UI

**Thread card** is bound to `run_id`, not parsed from markdown. Show it on that run’s agent comment or chat message when one exists; otherwise on the run row in the thread. Title, option names, Open. After Choose: Chosen badge. Not a mermaid fence.

**Sidebar:** the existing right-drawer pattern (ticket drawer / chat column). Tabs A/B/C, sandboxed iframe, Coppice **Choose**. Same chrome on desktop and web.

Iframe: `sandbox="allow-scripts"` without `allow-same-origin`. CSP: no network. Images only from the set folder (already in the blob tree). Treat HTML as untrusted.

Plugin off: tools gone on the next run. Existing cards still open read-only; Choose stays if the user can still comment.

### Next run

The next agent does not scrape comments. It calls `design__list`. `ticket_get` (ticket runs) and the chat turn payload include the same short summary: set title, option titles, chosen id + title. No HTML in the prompt. `chosen: null` means the agent may publish again or continue in text; the run is not blocked.

### Errors

Preview with missing files: empty state, no hang. Choose is never inside the iframe.

## Testing

- Create + publish → card payload; files only under `<artifacts>/designs/`.
- `..`, absolute entry, oversized folder, extra file types → rejected; no card.
- Choose → `design__list` and `ticket_get` show the pick; last pick wins.
- Second publish on the same run replaces the card; a new run adds a set and marks the previous superseded.
- Plugin off → `design__*` absent on the next run.
- Planning run still cannot write the worktree (existing check).
- One SPA test: card opens the sidebar, Choose posts, badge updates.

No connector-specific launch flags. No live-pause test.

## Out of scope

- Pausing a live CLI until the human picks (option B from brainstorming).
- Options that live only inside the agent HTML (postMessage picker).
- Writing designs into the git worktree or the operator’s `~/.cursor`.
- Electron `BrowserView` / `<webview>`.
- Screenshots, Figma import, or generating HTML for the agent.
- Changing plan approval, mermaid, or `result_submit`.
- A stdio child process for this plugin.
