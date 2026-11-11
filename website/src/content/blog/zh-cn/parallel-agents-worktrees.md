---
title: 如何用 Git worktree 并行运行多个 AI 编程智能体
description: "同时跑几个编程智能体，又不让它们互相改掉对方的文件：每个任务一个 git worktree，一段小脚本，或一块替你建好的看板。"
answer: "给每个智能体各自的 git worktree：同一仓库里的独立目录，各自一条分支，这样它们就不会改同一批文件。你可以手工用 git worktree add 创建，也可以包进一段小脚本，或者用 Coppice 这样的看板。看板上，每张工单开始工作时都会有自己的分支和 worktree，所有运行都在一处可见。无论哪种做法，每条分支都由你审核，再合并。"
published: "2026-11-07"
updated: "2026-11-09"
author: Coppice
authorType: Organization
draft: false
keywords:
  - git worktree
  - 并行智能体
  - 编程智能体
  - Claude Code
---

## 为什么难

两个智能体在同一个目录里会改同一批文件，一个会弄坏另一个的工作。Git worktree 解决的就是这件事：每个智能体有自己的目录和分支。

难的地方随后换了位置。每个智能体一个终端，你得记住哪个做完了、哪个在等你，还得一条一条合并分支。两件任务如果碰到同一批文件，合并时仍然可能冲突。

## 做法一：手工创建 worktree

每个任务建一个 worktree：

```bash
git worktree add ../my-project-login -b login
git worktree add ../my-project-search -b search
```

在每个目录里打开一个终端，并在那里启动智能体。任务做完后，审核并合并它的分支，再用 `git worktree remove` 删掉这个目录。

两三个任务时，这样做很好。再多，切换终端、跟踪每件任务，本身就成了工作。

## 做法二：包进一段小脚本

一个 shell 函数可以一步创建 worktree 并启动智能体：

```bash
agent_task() {
  dir="../$(basename "$PWD")-$1"
  git worktree add "$dir" -b "$1" && cd "$dir" && claude
}
```

在项目目录里运行 `agent_task login`。把 `claude` 换成 `codex`，或其他智能体 CLI。

这样省掉了准备时的输入。每个任务仍然是一个终端，跟踪和合并还是你来做。

## 做法三：用 Coppice，一块看板放下所有任务

[Coppice](/) 是一个桌面应用，把编程智能体放在一块任务看板上。工作开始时，每张工单都有自己的分支和 worktree，所以你的主检出目录不会被动到。多张工单可以同时跑，看板显示每张走到了哪一步。

1. [安装 Coppice](/docs/install)，并加入你的项目。
2. 创建你的智能体，并为每个智能体选择 Claude Code、Codex 或其他 CLI。
3. 每个任务写一张工单，然后让智能体开始做。
4. 批准每份计划，准备好之后再审核各自的 diff。
5. 一次合并一张工单。如果合并冲突，Coppice 会列出文件，你可以让一个智能体去解决。

![一张工单的 Git actions 区域，显示它自己的分支 agent/TICKET-723e8f0a，以及它的 worktree 目录。](/blog/parallel-agents-worktrees/branch-and-worktree.png "每张工单都有自己的分支和 worktree。")

[核心概念](/docs/concepts)说明了工单经过的每一步。

## 拆开任务，合并才干净

无论用哪种做法：

- 每件任务用自己的文件。lockfile、schema，或共享模块这类共用文件，归到一件任务，或归到后面的任务。
- 先合并一条分支，再把下一条更新到这个结果上，并检查两处改动放在一起是否仍然合理。Git 可以干净地合并两个文件，即使它们的内容互相矛盾。

## 该选哪一种

- **两三个任务：** 手工创建 worktree。
- **你经常开始任务，而且喜欢终端：** 一段小脚本。
- **同时有好几件任务，而且你想在一处查看、审核和合并：** 用 Coppice。从[快速上手](/docs/getting-started)开始。
