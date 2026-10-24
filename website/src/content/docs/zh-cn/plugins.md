---
title: 插件
metaTitle: "插件：智能体技能与 MCP 工具"
description: "插件为 Coppice 的智能体提供技能和 MCP 工具。从 git 或文件夹安装，启用后选择哪些智能体可以使用，也可以自己编写插件。"
---

# 插件

插件为智能体提供技能和 MCP 工具。添加插件、启用它，再选择哪些智能体可以使用。

一个插件就是一个文件夹，里面可以包含技能（`SKILL.md` 文件）和 MCP 工具（`.mcp.json`）。在侧边栏中打开 **Plugins**（插件）来管理它们。

内置的 `coppice` 插件始终存在，它的技能无法关闭。

<a id="install-a-plugin"></a>

## 安装插件
使用 **Add plugins**（添加插件）。有两种方式。

<a id="from-git"></a>

### 从 git 安装
1. 粘贴 **Git URL**。
2. 可选：设置 **Ref (optional)**，例如 `main`。
3. 选择 **Target directory**（目标目录）。
4. 点击 **Install**。

Coppice 会用你电脑上的 git 凭据克隆仓库。之后在卡片上点击 **Update** 即可拉取新改动。

<a id="from-a-folder"></a>

### 从文件夹添加
1. 在 **Plugin directories**（插件目录）下点击 **Browse…** 选择文件夹，或直接在 **Directory path** 中输入路径。
2. 点击 **Add directory**。

这个文件夹可以本身就是一个插件，也可以把多个插件作为子文件夹放在里面。修改文件后请点击 **Rescan**（重新扫描）。

如果两个文件夹里有同名插件，列表中靠上的那个优先。用箭头可以调整文件夹顺序。你的默认文件夹标有 **Default**，无法移除。

<a id="enable-it"></a>

## 启用插件
插件初始为禁用状态。

1. 打开插件卡片上的开关。只有状态为 `ok` 的插件才能启用。
2. 如果卡片上有 **Settings**，填写每个值并点击 **Save settings**。这些值会加密保存。
3. 点击 **Test**，启动插件的工具并列出它们。启用之前也可以测试。

带本地 MCP 服务器的插件会以你电脑的权限在本机运行。Coppice 会先请你确认。

每个技能都有自己的开关。关闭的技能对所有智能体都不可见，即使是已经在运行中的任务也一样。

<a id="choose-which-agents-can-use-it"></a>

## 选择哪些智能体可以使用
在卡片的 **Which agents can use it?**（哪些智能体可以使用？）下选择：

- **All agents**：所有智能体，包括以后新建的。
- **Choose agents**：只有你勾选的智能体。

也可以在智能体表单的 **Skills** 下选择插件。两处设置保持同步，改动从每个智能体的下一次运行开始生效。

新插件的初始设置如下：

- 只包含技能：**All agents**。
- 包含 MCP 服务器：**Choose agents**，且一个都没勾选。

<a id="use-it-on-a-ticket"></a>

## 在工单上使用
启用插件，并让工单的负责人有权使用它，然后点击 **Run agent**。

智能体能看到每个技能的名称和描述，需要时再加载该技能。工具的命名格式为 `<plugin>__<tool>`。

想查看用了什么，打开工单的 **Agent Runs**（智能体运行）标签页，展开一次运行，再打开 **Tools & Skills**。插件调用会标记为 **Plugin**。

<a id="create-a-plugin"></a>

## 创建插件
最简单的插件就是一个包含 `SKILL.md` 的文件夹。如果需要名称、版本和 MCP 工具，请添加清单文件。

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
放在 `.claude-plugin/plugin.json`。

```json
{
  "name": "hello-coppice",
  "version": "0.1.0",
  "description": "Example plugin: one skill and one MCP tool that greets people.",
  "author": { "name": "Coppice" }
}
```

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `name` | 是 | 字母、数字、`.`、`_` 和 `-`，最多 64 个字符，必须以字母或数字开头。`coppice` 为保留名称。 |
| `version` | 否 | 默认为 `0.0.0`。 |
| `description` | 否 | 默认为空。 |
| `author` | 否 | 字符串，或 `{ "name": "..." }`。 |
| `skills` | 否 | 一个技能目录的路径，或目录路径列表，相对于插件根目录。 |

没有 `plugin.json` 时，只要文件夹顶层有 `SKILL.md`，或者在 `skills/`、`.agents/skills/`、`.claude/skills/` 或 `<name>/SKILL.md` 中有技能，它仍然算一个插件。插件名取文件夹名，版本为 `0.0.0`。

<a id="skills"></a>

### 技能
每个技能是一个包含 `SKILL.md` 的文件夹，文件夹名就是技能名。文件以 frontmatter 开头：

```yaml
---
name: greeting-style
description: How to greet people in ticket comments. Load before writing a greeting.
---
```

`name` 和 `description` 为必填项。description 要写得让智能体知道什么时候该加载这个技能。

<a id="mcp-tools"></a>

### MCP 工具
添加 `.mcp.json`，在其中写一个 `mcpServers` 映射。每个服务器可以是本地的，也可以是远程的。

- 本地：`command`，以及可选的 `args` 和 `env`。
- 远程：`"type": "http"`、`url`，以及可选的 `headers`。

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

`${NAME}` 会成为插件卡片上的一项设置，`${NAME:-default}` 则为它提供默认值。Coppice 会把 `${CLAUDE_PLUGIN_ROOT}` 替换为插件的安装文件夹，它不是卡片上的设置。不支持 SSE 服务器。

这个示例提供了工具 `hello-coppice__greet` 和技能 `hello-coppice:greeting-style`。完整示例见 [`examples/plugins`](https://github.com/fertile-org/coppice/tree/main/examples/plugins)。

`commands/`、`agents/` 和 `hooks/` 文件夹暂不支持。它们会显示 "Not supported yet"，并且不会运行。
