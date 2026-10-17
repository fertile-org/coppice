[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文（台灣）](README.zh-TW.md) · [繁體中文（香港）](README.zh-HK.md) · [日本語](README.ja.md) · [Français](README.fr.md)

# Coppice

**Beta 版。** 雇用专家，你来拍板。

<p align="center">
  <img src="static/marketing.gif" alt="写一个工单，批准计划，审核 diff。你接受之前，什么都不算完成。" width="960" />
</p>

**不用再在各个智能体终端之间来回切换。** 写一个工单，你的 PM 智能体会起草一份计划等你批准。然后 Claude Code、Codex 等智能体并行构建、审查和测试，各自在自己的分支上工作。看板上能看到哪些在推进、哪些卡住了、哪些在等你；在你审核 diff 之前，什么都不算完成。它在你的电脑上运行，用的是你已经付费的智能体 CLI：不用新订阅，也不用 Coppice 账号。

## 两道关卡

工作会在两个地方停下来等你：

1. **Plan Review**（计划审核）。你的 PM 智能体起草计划，在开工之前由你批准或要求修改。规划永远不会改动工单的代码。开启 Skip planning（跳过规划）后，工单会直接进入 Ready。
2. **Wait for Human Review**（等待人工审核）。由你审核 diff。在你接受之前，工单会一直停在这一列。

工作由你发起。创建一个工单，或批准一个智能体建议的工单。在这两道关卡之间，由智能体负责规划和构建。

## 本次 Beta

Beta 版的安装包包括 macOS 的 `.dmg`（Apple 芯片或 Intel）和 Linux 的 `.deb`（`amd64` 或 `arm64`）。请从[发布页面](https://github.com/fertile-org/coppice/releases)下载。

带上你已经在用的 CLI：Claude Code、Codex、Cursor、OpenCode 和 Kilo Code。按常规方式安装并登录即可。Coppice 会在你登录 shell 的 `PATH` 中找到它们。

## 开始使用

产品文档就是 [`website/`](website/) 中的网站：落地页和用户指南。公开网站地址是 [https://getcoppice.vercel.app/](https://getcoppice.vercel.app/)。

## 贡献者

贡献者和 CI 都通过 Docker Compose 运行整套服务。请先阅读 [AGENTS.md](AGENTS.md) 和 [docs/development.md](docs/development.md)。路线图见 [docs/milestones/README.md](docs/milestones/README.md)。

## 许可证

Coppice 基于 [Apache 2.0 许可证](LICENSE)开源。
