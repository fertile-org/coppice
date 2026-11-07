---
title: 插件
metaTitle: "插件：智能体技能与 MCP 工具"
description: "插件为 Coppice 的智能体提供技能和 MCP 工具。从 git 或文件夹安装，启用后选择哪些智能体可以使用，也可以自己编写插件。"
---

# 插件

插件为智能体提供技能和 MCP 工具。一个插件就是一个文件夹，里面可以包含：

- **技能**：`SKILL.md` 文件。
- **MCP 工具**：一个 `.mcp.json` 文件。

在侧边栏中打开 **Plugins**（插件）来管理插件。内置的 `coppice` 插件始终存在，它的技能无法关闭。

<a id="install-a-plugin"></a>

## 1. 安装插件
**Plugins** 页面有三个标签页：

- **Installed**：你拥有的所有插件，无论来源。
- **Marketplace**：添加插件最快的方式。
- **Custom**：从 git 或文件夹安装。

<a id="from-the-marketplace"></a>

### 从插件市场安装
1. 打开 **Plugins**，切换到 **Marketplace** 标签页。
2. 搜索，或按来源、类别筛选。
3. 点击条目上的 **Install**。

<figure>
<img src="/assets/docs/plugin-marketplace.webp" width="1440" height="900" alt="Plugins 页面的 Marketplace 标签页，包含搜索框、来源筛选和带 Install 按钮的插件卡片。">
<figcaption>在数千个插件中搜索，一键安装。</figcaption>
</figure>

插件会出现在 **Installed** 标签页，处于禁用状态。接下来请[启用这个插件](#enable-it)。无论智能体运行在哪个 CLI 上，插件都能供你授权的智能体使用。

<a id="marketplace-sources"></a>

#### 来源
默认列出五个来源：

- **coppice**：Coppice 自带的目录。
- **claude-plugins-official**：Anthropic 的官方目录。
- **claude-community**：Anthropic 的社区目录。
- **superpowers-marketplace**
- **ponytail**

来源的加载方式：

- 第一次打开 **Marketplace** 标签页时，每个来源会自动加载。
- 如果来源已超过 24 小时，Coppice 会在后台重新加载。
- 加载失败的来源标签会显示错误，点击该标签可重试。

<a id="community-plugins"></a>

#### 谁审核过插件
- 标记为 **Coppice** 的卡片来自 Coppice 目录。
- 其他卡片都标记为 **Community: not reviewed by Coppice**。启用前请先审核。
- 固定了 `sha` 的目录条目会安装那个确切的提交，即使分支已经变化。

<a id="manage-sources"></a>

#### 管理来源
点击 **Manage sources**，查看每个来源及其上次刷新时间。

- **添加来源**：添加一个包含 `.claude-plugin/marketplace.json` 的 git 仓库。Coppice 会立即获取它的目录。
- **重新获取**：点击 **Refresh**，出错后点击 **Retry**。
- **移除来源**：除 **coppice** 外都可以移除。从该来源安装的插件会保留。移除的默认来源不会再回来。

<a id="example-github"></a>

#### 示例：GitHub
1. 从插件市场安装 `github`。
2. 打开这个插件，在 **Settings** 下把 GitHub 个人访问令牌粘贴到 `GITHUB_PERSONAL_ACCESS_TOKEN`，然后点击 **Save settings**。
3. 点击 **Test**，列出这个插件的工具。
4. 启用这个插件，并选择哪些智能体可以使用。

这个令牌与 **Settings** 里 Coppice 用于拉取请求的 GitHub 令牌是分开的。

<a id="from-git"></a>

### 从 git 安装
1. 打开 **Custom** 标签页。
2. 粘贴 **Git URL**。
3. 可选：设置 **Ref (optional)**，例如 `main`。
4. 选择 **Target directory**（目标目录）。
5. 点击 **Install**。

Coppice 会用你电脑上的 git 凭据克隆仓库。之后在详情里点击 **Update** 即可拉取新改动。

<a id="from-a-folder"></a>

### 从文件夹安装
1. 打开 **Custom** 标签页。
2. 在 **Plugin directories**（插件目录）下点击 **Browse…** 选择文件夹，或直接在 **Directory path** 中输入路径。
3. 点击 **Add directory**。

- 这个文件夹可以本身就是一个插件，也可以把多个插件作为子文件夹放在里面。
- 修改文件后请点击 **Rescan**（重新扫描）。
- 如果两个文件夹里有同名插件，列表中靠上的那个优先。用箭头可以调整文件夹顺序。
- 你的默认文件夹标有 **Default**，无法移除。

<a id="enable-it"></a>

## 2. 启用插件
插件初始为禁用状态。

1. 打开插件行上的开关。只有状态为 `ok` 的插件才能启用。
2. 如果详情里有 **Settings**，填写每个值并点击 **Save settings**。这些值会加密保存。
3. 点击 **Test**，启动插件的工具并列出它们。启用之前也可以测试。

- 带本地 MCP 服务器的插件会以你电脑的权限在本机运行。Coppice 会先请你确认。
- 每个技能都有自己的开关。关闭的技能对所有智能体都不可见，即使是已经在运行中的任务也一样。

<a id="choose-which-agents-can-use-it"></a>

## 3. 选择哪些智能体可以使用
在详情的 **Which agents can use it?**（哪些智能体可以使用？）下选择：

- **All agents**：所有智能体，包括以后新建的。
- **Choose agents**：只有你勾选的智能体。

新插件的初始设置如下：

- 只包含技能：**All agents**。
- 包含 MCP 服务器：**Choose agents**，且一个都没勾选。

- 也可以在智能体表单的 **Skills** 下选择插件。两处设置保持同步。
- 改动从每个智能体的下一次运行开始生效。

<a id="use-it-on-a-ticket"></a>

## 4. 在工单上使用
1. 启用插件。
2. 让工单的负责人有权使用它。
3. 点击 **Run agent**。

- 智能体能看到每个技能的名称和描述，需要时再加载该技能。
- 工具的命名格式为 `<plugin>__<tool>`。

想查看用了什么：

1. 打开工单的 **Agent Runs**（智能体运行）标签页。
2. 展开一次运行。
3. 打开 **Tools & Skills**。插件调用会标记为 **Plugin**。

<a id="create-a-plugin"></a>

## 5. 创建插件
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

没有 `plugin.json` 时，只要满足以下任一条件，文件夹仍然算一个插件：

- 顶层有 `SKILL.md`。
- 在 `skills/`、`.agents/skills/`、`.claude/skills/` 或 `<name>/SKILL.md` 中有技能。

插件名取文件夹名，版本为 `0.0.0`。

<a id="skills"></a>

### 技能
每个技能是一个包含 `SKILL.md` 的文件夹，文件夹名就是技能名。文件以 frontmatter 开头：

```yaml
---
name: greeting-style
description: How to greet people in ticket comments. Load before writing a greeting.
---
```

- `name` 和 `description` 为必填项。
- description 要写得让智能体知道什么时候该加载这个技能。

<a id="mcp-tools"></a>

### MCP 工具
添加 `.mcp.json`，在其中写一个 `mcpServers` 映射。每个服务器可以是本地的，也可以是远程的：

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

- `${NAME}` 会成为插件详情里的一项设置。
- `${NAME:-default}` 为这项设置提供默认值。
- Coppice 会把 `${CLAUDE_PLUGIN_ROOT}` 替换为插件的安装文件夹，它不是一项设置。
- 不支持 SSE 服务器。

这个示例提供了工具 `hello-coppice__greet` 和技能 `hello-coppice:greeting-style`。完整示例见 [`examples/plugins`](https://github.com/fertile-org/coppice/tree/main/examples/plugins)。

`commands/`、`agents/` 和 `hooks/` 文件夹暂不支持。它们会显示 "Not supported yet"，并且不会运行。
