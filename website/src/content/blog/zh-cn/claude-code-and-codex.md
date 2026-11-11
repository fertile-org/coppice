---
title: 如何在同一个项目里一起用 Claude Code 和 Codex
metaTitle: 同一个项目里用 Claude Code 和 Codex
description: "在同一份代码上跑 Claude Code 和 Codex，有三种做法：两个终端、git worktree，或一块看板，让每个智能体做自己的工单。"
answer: "给每个工具一份自己的代码副本，这样它们就不会同时改同一批文件。你可以自己用 git worktree 分开，也可以用 Coppice 这样的看板：每张工单有自己的分支，由你决定这张工单用 Claude Code 还是 Codex。无论哪种，改动进主分支之前都要你审核。"
published: "2026-11-07"
updated: "2026-11-08"
draft: false
author: Coppice
authorType: Organization
keywords:
  - Claude Code
  - Codex
  - 编程智能体
  - git worktree
---

## 为什么要两个一起用

Claude Code 和 Codex 都是在终端里跑的编程智能体。你可以让一个做计划、另一个写代码，也可以让第二个模型去审第一个改过的内容。难的不是把它们跑起来，而是把两边的改动分开，这样你才看得出是谁改了什么。

常见有三种做法。

## 做法一：两个终端，同一份检出

在两个终端里打开同一个项目。一个运行 `claude`，另一个运行 `codex`。

这是上手最快的办法。一次只用一个工具时很合适，比如用 Codex 去审 Claude Code 刚写完的内容。如果两个同时改，它们可能改到同一个文件，后写的会盖掉先写的。

## 做法二：每个工具一个 git worktree

git worktree 是同一仓库的第二个工作目录，在自己的分支上。给 Codex 建一个：

```bash
git worktree add ../my-project-codex -b codex-task
```

在主目录里运行 `claude`，在 `../my-project-codex` 里运行 `codex`。两边可以同时干活，不会碰到对方的文件。做完之后，你自己审核每个分支，再自己合并。

这样改动是分开的，但协调还是你来做：哪件任务交给哪个工具、两边怎么交接上下文、分支用完怎么清掉。

## 做法三：用 Coppice，一块看板放两个

[Coppice](/) 是一个桌面应用，把编程智能体放在一块任务看板上。它跑的是你电脑上已经有的 `claude` 和 `codex` 命令。每张工单在自己的分支上跑，主检出目录不会被动到，智能体通过看板把工单交给彼此。

1. [安装 Coppice](/docs/install)。在终端里按你平时的方式登录 Claude Code 和 Codex。
2. 把项目文件夹加到仓库列表里。
3. 创建智能体，每个选 Claude Code 或 Codex。比如 PM 和 Technical Lead 用 Claude Code，Backend Engineer 用 Codex，Reviewer 用 Claude Code，这样 Claude Code 去审 Codex 写的内容。
4. 写好工单，让智能体开始做。

PM 写出计划，由你批准。然后智能体构建并审查这次改动。你读最终的 diff，接受它，准备好了再合并。

![新建智能体的画面。已选 PM 预设，智能体的 CLI 选的是 Claude Code。](/blog/claude-code-and-codex/new-agent.png "每个智能体只用一个 CLI。创建时选择 Claude Code 或 Codex。")

## 该选哪一种

- **事情不大，一次只用一个工具：** 两个终端。
- **要并行，而且你愿意自己管分支：** git worktree。
- **工单很多，智能体互相交接和审查，计划和 diff 都由你批准：** 用 Coppice。从[快速上手](/docs/getting-started)开始。
