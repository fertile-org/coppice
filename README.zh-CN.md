[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文（台灣）](README.zh-TW.md) · [繁體中文（香港）](README.zh-HK.md) · [日本語](README.ja.md) · [Français](README.fr.md)

<p align="center">
  <img src="website/public/logo.svg" alt="" width="72" />
</p>

<h1 align="center">Coppice</h1>

<p align="center">
  <strong>雇用专家，你来拍板。</strong><br />
  各有角色的编程智能体在同一块看板上规划、构建和审查，你接受 diff 之前，什么都不算完成。<br />
  本地运行，开源，用的是你已经付费的智能体 CLI。
</p>

<p align="center">
  <a href="https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-arm64.dmg"><strong>下载 macOS 版（Apple Silicon）</strong></a> ·
  <a href="https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-amd64.deb"><strong>下载 Linux 版（.deb）</strong></a> ·
  <a href="https://getcoppice.app/zh-cn/download">其他下载</a> ·
  <a href="https://getcoppice.app/zh-cn/docs">文档</a>
</p>

<p align="center">
  <a href="https://github.com/fertile-org/coppice/releases"><img src="https://img.shields.io/github/v/release/fertile-org/coppice?include_prereleases&label=beta" alt="最新版本" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache%202.0-blue" alt="Apache 2.0 许可证" /></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows%20(WSL)-lightgrey" alt="macOS、Linux，以及通过 WSL 的 Windows" />
</p>

<p align="center">
  <img src="static/marketing.gif" alt="写一个工单，批准 PM 智能体起草的计划，让智能体构建和测试，然后评论 diff。你接受之前，什么都不算完成。" width="960" />
</p>

你的 PM 智能体起草计划，由你批准。专业智能体并行构建，每个都在自己的分支上。看板显示哪些在推进、哪些卡住了、哪些在等你。你接受 diff 之前，什么都不算完成。

下载、打开、开干。无需任何配置：不用 Docker，也不用 Coppice 账号。

## 为什么选择 Coppice

- **每个角色都用合适的模型。** 每个智能体各自选择 CLI 和模型。用最强的模型做规划和审查，用更快的模型写代码，全都在同一块看板上。
- **不用再在终端之间来回切换。** 每个智能体的工作都是同一块看板上的一张卡片。受阻的工单和等你审核的工作一目了然，你知道哪里需要你。
- **在落地之前发现错误。** 每个改动都会以 diff 的形式在应用里等你审核。你接受之前，什么都不算完成；你接受的内容，就是最终合并的内容。
- **不用再多一份订阅。** Coppice 驱动的是你已经付费的 CLI，登录方式和你平时一样。

## 工作方式

1. **把工作交出去。** 写一个工单，或批准智能体建议的工单。把它指派给你的 PM 智能体，它会起草一份计划，经你批准后才会有人写代码。打开 **Skip planning**（跳过规划），工单会直接进入 Ready。
2. **让智能体去构建。** 每个工单都有自己的 git worktree，并行的智能体永远不会碰你的主检出目录。任何一次运行都可以实时查看。
3. **人工审核。** 在应用里阅读 diff，可以评论任意一行。你接受之前，什么都不算完成。

<table>
  <tr>
    <td width="33%"><img src="static/screenshots/07-board.png" alt="看板：Backlog、Plan Review、Ready 和 In Progress 列，以及工单卡片" /></td>
    <td width="33%"><img src="static/screenshots/02-plan.png" alt="PM 智能体在工单上起草的计划：方法、步骤、风险和不在范围内的内容" /></td>
    <td width="33%"><img src="static/screenshots/05-diff.png" alt="代码审查：已批准的计划、改动的文件，以及带行内评论的 diff" /></td>
  </tr>
  <tr>
    <td align="center">所有智能体，一块看板</td>
    <td align="center">先批准计划</td>
    <td align="center">逐行审查 diff</td>
  </tr>
</table>

## 用你已经在用的 CLI

按常规方式安装并登录即可。Coppice 会在你登录 shell 的 `PATH` 中找到它们。

| CLI | 命令 |
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

安装链接和各个 CLI 支持的功能，见 [提供商](https://getcoppice.app/zh-cn/docs/providers)。

## 还有这些

- **和你的智能体聊天。** 聊聊你的代码、一个想法或一个工单。智能体可以起草工单或建议一条 Knowledge 笔记，但你批准之前什么都不会保存。
- **一切都在卡片上。** 评论、计划和智能体运行都留在工单上。智能体之间只通过工单评论交流，所以发生过的事，你都能在那里读到。
- **Knowledge。** 智能体会为以后的工单记下关于你项目的笔记。默认由你批准它们保留什么。
- **插件。** 给智能体技能和 MCP 工具。从 git 或文件夹安装插件，再选择哪些智能体可以使用。
- **自动更新。** Linux 版通过 apt 更新，macOS 版由应用自己更新。

## 它不是什么

- 不是云端编程服务
- 不是 Jira 的替代品
- 不是“告诉智能体一句，就拿到成品”

## 开始使用

1. [下载 Coppice](https://getcoppice.app/zh-cn/download)：macOS 用 `.dmg`（Apple Silicon 或 Intel），Linux 用 `.deb`（`amd64` 或 `arm64`），或者通过 WSL 在 Windows 11 上运行。
2. 按照 [安装步骤](https://getcoppice.app/zh-cn/docs/install) 操作。
3. 阅读 [入门](https://getcoppice.app/zh-cn/docs/getting-started)，设置你的第一个智能体和工单。

**Beta。** Coppice 还在测试阶段。遇到出错或不对劲的地方，请 [提交 issue](https://github.com/fertile-org/coppice/issues)。

## 贡献者

贡献者和 CI 使用 Docker Compose 运行整套服务。先读 [AGENTS.md](AGENTS.md) 和 [docs/development.md](docs/development.md)。路线图在 [docs/milestones/README.md](docs/milestones/README.md)。产品文档是 [`website/`](website/) 里的网站。

## 许可证

Coppice 以 [Apache 2.0 许可证](LICENSE) 开源。
