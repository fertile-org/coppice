---
layout: ../../layouts/Docs.astro
title: Plugins
metaTitle: "Plugins: Agent Skills and MCP Tools"
description: "Plugins give Coppice agents skills and MCP tools. Install one from the marketplace, git or a folder, enable it, pick which agents use it, or write your own."
---

# Plugins

Plugins give agents skills and MCP tools. A plugin is a folder that can hold:

- **Skills**: `SKILL.md` files.
- **MCP tools**: a `.mcp.json` file.

Open **Plugins** in the sidebar to manage them. The built-in `coppice` plugin is always there, and you can't switch its skills off.

<a id="install-a-plugin"></a>

## 1. Install a plugin
The **Plugins** page has three tabs:

- **Installed**: every plugin you have, wherever it came from.
- **Marketplace**: the quickest way to add one.
- **Custom**: install from git or a folder.

<a id="from-the-marketplace"></a>

### From the marketplace
1. Open **Plugins** and switch to the **Marketplace** tab.
2. Search, or filter by source or category.
3. Click **Install** on an entry.

<figure>
<img src="/assets/docs/plugin-marketplace.webp" width="1440" height="900" alt="The Marketplace tab on the Plugins page, with search, source filters and plugin cards with Install buttons.">
<figcaption>Search thousands of plugins, and install one with a click.</figcaption>
</figure>

The plugin appears on the **Installed** tab, disabled. Next, [enable it](#enable-it). A plugin works for every agent you give it to, whatever CLI that agent runs on.

<a id="marketplace-sources"></a>

#### Sources
Five sources are listed out of the box:

- **coppice**: the catalog that ships with Coppice.
- **claude-plugins-official**: Anthropic's official directory.
- **claude-community**: Anthropic's community directory.
- **superpowers-marketplace**
- **ponytail**

How sources load:

- Each source loads on its own the first time you open the **Marketplace** tab.
- That source loads again in the background if it is older than 24 hours.
- A failed source chip shows an error. Click the chip to retry that source.

<a id="community-plugins"></a>

#### Who reviewed a plugin
- Cards marked **Coppice** come from the Coppice catalog.
- Every other card is marked **Community: not reviewed by Coppice**. Review these before you enable them.
- A catalog entry that pins a `sha` installs that exact commit, even if the branch has moved.

<a id="manage-sources"></a>

#### Manage sources
Click **Manage sources** to see each source and its last refresh.

- **Add a source**: add a git repository that has a `.claude-plugin/marketplace.json`. Coppice fetches its catalog right away.
- **Fetch again**: click **Refresh**, or **Retry** after an error.
- **Remove a source**: you can remove any source except **coppice**. Plugins you installed from it stay installed. A removed default source does not come back.

<a id="example-github"></a>

#### Example: GitHub
1. Install `github` from the marketplace.
2. Open it, paste a GitHub personal access token into `GITHUB_PERSONAL_ACCESS_TOKEN` under **Settings**, and click **Save settings**.
3. Click **Test** to list its tools.
4. Enable it and pick the agents that can use it.

This token is separate from the GitHub token in **Settings** that Coppice uses for pull requests.

<a id="from-git"></a>

### From git
1. Open the **Custom** tab.
2. Paste the **Git URL**.
3. Optional: set **Ref (optional)**, such as `main`.
4. Pick a **Target directory**.
5. Click **Install**.

Coppice clones the repo with your computer's git credentials. Click **Update** in its details later to pull new changes.

<a id="from-a-folder"></a>

### From a folder
1. Open the **Custom** tab.
2. Under **Plugin directories**, click **Browse…** and pick a folder. Or type it in **Directory path**.
3. Click **Add directory**.

- The folder can be one plugin, or hold several plugins as subfolders.
- Click **Rescan** after you change files.
- If two folders have a plugin with the same name, the one higher in the list wins. Use the arrows to reorder folders.
- Your default folder shows **Default** and can't be removed.

<a id="enable-it"></a>

## 2. Enable it
Plugins start disabled. If a plugin has a local MCP server, that server runs on your computer with your computer's privileges. Coppice asks you to confirm first.

1. Turn on the switch on the plugin's row. Only plugins with status `ok` can be enabled.
2. If its details have **Settings**, fill in each value and click **Save settings**. Values are stored encrypted.
3. Click **Test** to start the plugin's tools and list them. This works before you enable it.

- Each skill has its own switch. A skill you turn off is hidden from every agent, even in runs already going.

<a id="choose-which-agents-can-use-it"></a>

## 3. Choose which agents can use it
In its details, under **Which agents can use it?**, pick:

- **All agents**: every agent, including ones you create later.
- **Choose agents**: only the agents you check.

New plugins start like this:

- Skills only: **All agents**.
- Has an MCP server: **Choose agents**, with nobody checked.

- You can also pick plugins on the agent form, under **Skills**. Both places stay in sync.
- Changes apply from each agent's next run.

<a id="use-it-on-a-ticket"></a>

## 4. Use it on a ticket
1. Enable the plugin.
2. Give the ticket's assignee access.
3. Click **Run agent**.

- The agent sees each skill's name and description, and loads a skill when it needs it.
- Tools are named `<plugin>__<tool>`.

To see what was used:

1. Open the ticket's **Agent Runs** tab.
2. Expand a run.
3. Open **Tools & Skills**. Plugin calls are marked **Plugin**.

<a id="create-a-plugin"></a>

## 5. Create a plugin
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

Without `plugin.json`, a folder is still a plugin if it has either:

- A `SKILL.md` at the top.
- Skills in `skills/`, `.agents/skills/`, `.claude/skills/`, or `<name>/SKILL.md`.

The plugin takes the folder's name and version `0.0.0`.

<a id="skills"></a>

### Skills
Each skill is a folder with a `SKILL.md`. The folder name is the skill name. The file starts with frontmatter:

```yaml
---
name: greeting-style
description: How to greet people in ticket comments. Load before writing a greeting.
---
```

- `name` and `description` are required.
- Write the description so the agent knows when to load the skill.

<a id="mcp-tools"></a>

### MCP tools
Add `.mcp.json` with an `mcpServers` map. Each server is local or remote:

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

- `${NAME}` becomes a setting in the plugin's details.
- `${NAME:-default}` gives that setting a default.
- `${CLAUDE_PLUGIN_ROOT}` is filled in by Coppice with the plugin's install folder. It is not a setting.
- SSE servers aren't supported.

This example exposes the tool `hello-coppice__greet` and the skill `hello-coppice:greeting-style`. See the full example in [`examples/plugins`](https://github.com/fertile-org/coppice/tree/main/examples/plugins).

`commands/`, `agents/` and `hooks/` folders aren't supported yet. They show "Not supported yet" and don't run.
