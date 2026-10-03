# Plugins

Plugins add **skills** and **tools** to Coppice agents. This guide covers using plugins from the **Plugins** page (sidebar, admins only) and writing your own. For how the gateway and proxy work inside the server, see [architecture.md](architecture.md) (MCP gateway, plugin MCP proxy).

## What a plugin is

A plugin is a folder in the Claude Code / Cursor plugin format. It can contain:

| Part | File | What the agent gets |
|------|------|---------------------|
| Skills | `skills/<name>/SKILL.md` | Markdown instructions. The agent sees each skill's name and description and loads the full text with `skill_load` only when it needs it. |
| Tools | `.mcp.json` | MCP servers the plugin starts (stdio) or connects to (HTTP). Their tools appear to the agent as `<plugin>__<tool>`. |
| Settings | `${VAR}` placeholders in `.mcp.json` | Values a tool needs (API keys, paths). Entered on the plugin card, stored encrypted, never shown again. |

A plugin is not a rule set and does not change workflow, statuses, or permissions. Coppice's own rules (collaboration, splitting, role stages) ship as the built-in `coppice` plugin, which every agent always has.

`commands/`, `agents/`, and `hooks/` folders are detected and listed as **Not supported yet**; they never run.

## Using plugins

### 1. Add a plugin

On **Plugins → Add plugins**, either:

- **Install from git:** paste a repository URL (and optionally a branch or tag) and pick a target directory. The server clones it with its own git credentials. Git-installed plugins get an **Update** button.
- **Add a plugin directory:** a folder on the server's machine. Coppice scans the folder itself (if it is a plugin) and each direct subfolder (one plugin per subfolder). Deeper folders are ignored. Press **Rescan** after changing files. In the desktop app, **Browse…** opens a folder picker.

Directories are scanned in order. If two plugins have the same name, the one in the earlier directory is used and the other is marked `shadowed`. Use the arrows to reorder.

To try the flow, add the [`examples/plugins`](../examples/plugins) folder from this repository. It contains `hello-coppice`, which has one skill and one tool. It needs `node` on the server's PATH.

### 2. Enable it and fill in settings

Plugins start **disabled**. On the plugin card:

1. If the card lists **Settings**, enter the values and **Save**. Each setting shows where its value would come from: `setting` (saved here), `env` (the server's environment variable of the same name), `default` (the plugin's built-in default), or `missing` (the server will not start until you set it).
2. Press **Test** to start the plugin's MCP servers and list their tools. This works while disabled.
3. Turn on the switch to enable it.

A plugin with **stdio** servers runs a program on the server's machine with the server's privileges (sandboxing comes in M11). Enabling or testing such a plugin asks for confirmation, and also names any settings whose value would come from the server's environment. Only install plugins you trust.

### 3. Attach it to agents

Enabling does not give the plugin to anyone. Click **Give it to an agent →** on the plugin card: it opens **Agents** with the plugin pre-selected, so you click **Edit** on each agent that should have it and **Save**. (Or open **Agents** yourself, edit an agent, and select the plugin under **Plugins**.) Only enabled plugins with status `ok` can be selected. Agent presets may select some plugins by default.

What each kind of run gets from attached plugins:

| Run | Skills | Tools |
|-----|--------|-------|
| Ticket runs | yes | all |
| Chat (human chat, conversation) | yes | read-only tools only (`readOnlyHint`) |
| Knowledge compaction | yes | no |
| Connector checks (Tools → Connectors) | no | no |

### 4. Check that it is used

Open a ticket's **Runs** tab and expand a run. **Tools & Skills** lists every tool call (plugin calls show the plugin name) and the skills the agent loaded. The live console titles plugin calls `<plugin> · <tool>`.

### Status and health

| Plugin status | Meaning |
|---------------|---------|
| `ok` | Loaded; can be enabled and attached. |
| `shadowed` | A plugin with the same name in an earlier directory is used instead. |
| `missing` | The folder is gone. Agent assignments are kept and return when the folder does. |
| `invalid` | The plugin could not be read; the card shows the error. |

| Server health | Meaning |
|---------------|---------|
| `stopped` | Not running. Starts on first use (a run or Test) and stops after being idle. |
| `starting` | Starting. |
| `ready` | Running; tools available. |
| `backoff` | Failed recently; retried with increasing delay. |
| `unhealthy` | Failed three times in a row; its tools are hidden until you fix the cause and press **Test**. |

### Troubleshooting

| Symptom | Fix |
|---------|-----|
| Plugin does not appear | Is it directly in a plugin directory (or one level down)? Press **Rescan**. |
| `invalid` with a JSON error | Fix `plugin.json` or `.mcp.json` and rescan. |
| A skill shows an error | `SKILL.md` needs frontmatter with both `name` and `description`. The rest of the plugin still works. |
| Test says `missing setting "X"` | Enter setting `X` on the card, or give it a default in `.mcp.json`. |
| Test fails to start a stdio server | Is the command installed and on the server's PATH (in Docker: inside the server container)? |
| Agent never uses the plugin | Is it enabled **and** selected on that agent? Check the run's **Tools & Skills** tab. |

## Writing a plugin

Start from [`examples/plugins/hello-coppice`](../examples/plugins/hello-coppice):

```text
hello-coppice/
├── .claude-plugin/
│   └── plugin.json
├── .mcp.json
├── server/
│   └── index.mjs
└── skills/
    └── greeting-style/
        └── SKILL.md
```

### `plugin.json`

```json
{
  "name": "hello-coppice",
  "version": "0.1.0",
  "description": "Example plugin: one skill and one MCP tool that greets people.",
  "author": { "name": "Coppice" }
}
```

`name` must be unique across your plugin directories; it prefixes every tool (`hello-coppice__greet`) and skill (`hello-coppice:greeting-style`). Keep it short, lowercase, with `-` or `_`.

**Skills-only folder:** a folder with `<name>/SKILL.md` files (or a `skills/` subfolder) and no `plugin.json` also works. Its name is the folder name and its version is `0.0.0`.

### Skills

```markdown
---
name: greeting-style
description: How to greet people in ticket comments. Load before writing a greeting.
---

# Greeting style

1. Call the `hello-coppice__greet` tool with the person's name.
...
```

- Frontmatter `name` and `description` are required, as flat `key: value` lines.
- The description is all the agent sees until it loads the skill, so say **when** to use it.
- Keep the body focused on one task. Refer to tools by their full `<plugin>__<tool>` name.

### MCP servers (`.mcp.json`)

**Local (stdio):** Coppice starts the command and talks MCP over stdin/stdout.

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

- The working directory is the plugin folder; `${CLAUDE_PLUGIN_ROOT}` is its absolute path.
- The process gets a minimal environment: `PATH`, `HOME`, `LANG`, `TMPDIR` from the server, plus the entry's `env`. Pass everything else through `env`.
- stdout is for MCP messages only; log to stderr.

**Remote (HTTP):** streamable HTTP MCP endpoints.

```json
{
  "mcpServers": {
    "api": {
      "type": "http",
      "url": "https://example.com/mcp",
      "headers": { "Authorization": "Bearer ${API_KEY}" }
    }
  }
}
```

SSE-only servers (`"type": "sse"`) are listed as unsupported. `mcpServers` may also sit inside `plugin.json`; if both exist, `.mcp.json` wins.

### Settings and placeholders

Every `${NAME}` in `command`, `args`, `env` values, `url`, or `headers` values becomes a setting on the plugin card. When the server starts, each is resolved in order:

1. the value saved on the card;
2. the server's environment variable `NAME` (never names starting with `COPPICE_`, nor `DATABASE_URL` or `SECRETS_MASTER_KEY`);
3. the default in `${NAME:-default}`;
4. otherwise the server fails to start with `missing setting "NAME"`.

Use a default for optional values; leave secrets without one so the admin must set them.

### Tools

- Tool names should be short and stable. The exposed name `<plugin>__<tool>` is sanitized to letters, digits, `_`, `-`; long names are shortened with a hash.
- Write a clear `description` and `inputSchema`; that is what the agent reads.
- Mark tools that only read with `"annotations": { "readOnlyHint": true }`. Only read-only tools are offered in chat.
- Return errors as a tool result with `"isError": true` and a readable message.
- Coppice applies a per-call timeout and output size limit; keep calls quick and output small.

### Develop and test locally

1. Put your plugin in a folder and add that folder (or its parent) as a plugin directory.
2. Press **Test** on the card after each change to `.mcp.json` or the server code; it restarts the plugin's servers. Press **Rescan** after adding or renaming skills or editing `plugin.json`.
3. Attach the plugin to an agent and run a ticket. Check **Tools & Skills** on the run.

You can also talk to a stdio server by hand:

```sh
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"greet","arguments":{"name":"Ada"}}}' \
  | node examples/plugins/hello-coppice/server/index.mjs
```

### Share it

Push the plugin folder as the root of a git repository. Others install it with **Install from git** and pick up new commits with **Update**. Plugins also work unchanged in Claude Code and Cursor if they only use skills and MCP servers.
