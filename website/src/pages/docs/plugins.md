---
layout: ../../layouts/Docs.astro
title: Plugins
description: "Plugins give agents skills and MCP tools. Add one, enable it, and choose which agents can use it."
---

# Plugins

Plugins give agents skills and MCP tools. Add one, enable it, and choose which agents can use it.

A plugin is a folder. It can hold skills (`SKILL.md` files) and MCP tools (`.mcp.json`). Open **Plugins** in the sidebar to manage them.

The built-in `coppice` plugin is always there. You can't switch its skills off.

## Install a plugin
Use **Add plugins**. There are two ways.

### From git
1. Paste the **Git URL**.
2. Optional: set **Ref (optional)**, such as `main`.
3. Pick a **Target directory**.
4. Click **Install**.

Coppice clones the repo with your computer's git credentials. Click **Update** on the card later to pull new changes.

### From a folder
1. Under **Plugin directories**, click **Browse…** and pick a folder. Or type it in **Directory path**.
2. Click **Add directory**.

The folder can be one plugin, or hold several plugins as subfolders. Click **Rescan** after you change files.

If two folders have a plugin with the same name, the one higher in the list wins. Use the arrows to reorder folders. Your default folder shows **Default** and can't be removed.

## Enable it
Plugins start disabled.

1. Turn on the switch on the plugin's card. Only plugins with status `ok` can be enabled.
2. If the card has **Settings**, fill in each value and click **Save settings**. Values are stored encrypted.
3. Click **Test** to start the plugin's tools and list them. This works before you enable it.

A plugin with a local MCP server runs on your computer with your computer's privileges. Coppice asks you to confirm first.

Each skill has its own switch. A skill you turn off is hidden from every agent, even in runs already going.

## Choose which agents can use it
On the card, under **Which agents can use it?**, pick:

- **All agents**: every agent, including ones you create later.
- **Choose agents**: only the agents you check.

You can also pick plugins on the agent form, under **Skills**. Both places stay in sync. Changes apply from each agent's next run.

New plugins start like this:

- Skills only: **All agents**.
- Has an MCP server: **Choose agents**, with nobody checked.

## Use it on a ticket
Enable the plugin and give the ticket's assignee access. Then click **Run Agent**.

The agent sees each skill's name and description, and loads a skill when it needs it. Tools are named `<plugin>__<tool>`.

To see what was used, open the ticket's **Agent Runs** tab, expand a run, and open **Tools & Skills**. Plugin calls are marked **Plugin**.

## Create a plugin
The simplest plugin is a folder with a `SKILL.md`. For a name, version and MCP tools, add a manifest.

```
hello-coppice/
├── .claude-plugin/
│   └── plugin.json
├── skills/
│   └── greeting-style/
│       └── SKILL.md
└── .mcp.json
```

### `plugin.json`
Put it at `.claude-plugin/plugin.json`.

```json
{
  "name": "hello-coppice",
  "version": "0.1.0",
  "description": "Example plugin: one skill and one MCP tool that greets people.",
  "author": { "name": "Coppice" }
}
```

| Field | Required | Notes |
| --- | --- | --- |
| `name` | Yes | Letters, numbers, `.`, `_` and `-`, up to 64 characters. Starts with a letter or number. `coppice` is reserved. |
| `version` | No | Defaults to `0.0.0`. |
| `description` | No | Defaults to empty. |
| `author` | No | A string, or `{ "name": "..." }`. |
| `skills` | No | A path to a skill directory, or a list of directory paths, relative to the plugin root. |

Without `plugin.json`, a folder is still a plugin if it has a `SKILL.md` at the top, or skills in `skills/`, `.agents/skills/`, `.claude/skills/`, or `<name>/SKILL.md`. It takes the folder's name and version `0.0.0`.

### Skills
Each skill is a folder with a `SKILL.md`. The folder name is the skill name. The file starts with frontmatter:

```yaml
---
name: greeting-style
description: How to greet people in ticket comments. Load before writing a greeting.
---
```

`name` and `description` are required. Write the description so the agent knows when to load the skill.

### MCP tools
Add `.mcp.json` with an `mcpServers` map. Each server is local or remote.

- Local: `command`, plus optional `args` and `env`.
- Remote: `"type": "http"`, `url`, plus optional `headers`.

```json
{
  "mcpServers": {
    "hello": {
      "command": "node",
      "args": ["${CLAUDE_PLUGIN_ROOT}/server/index.mjs"],
      "env": { "HELLO_GREETING": "${HELLO_GREETING:-Hello}" }
    }
  }
}
```

`${NAME}` becomes a setting on the plugin's card. `${NAME:-default}` gives it a default. Coppice fills in `${CLAUDE_PLUGIN_ROOT}` with the plugin's install folder. It is not a card setting. SSE servers aren't supported.

This example exposes the tool `hello-coppice__greet` and the skill `hello-coppice:greeting-style`. See the full example in [`examples/plugins`](https://github.com/fertile-org/coppice/tree/main/examples/plugins).

`commands/`, `agents/` and `hooks/` folders aren't supported yet. They show "Not supported yet" and don't run.
