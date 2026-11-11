---
title: 如何用 Fable 做规划，用更便宜的模型写代码
metaTitle: 用 Fable 规划，用便宜模型写代码
description: "规划用 Fable 5.2，写代码用 Opus 或 Sonnet。Claude Code 没有先 Fable 再换便宜模型的设置。Coppice 可以给每个智能体单独设模型。"
answer: "规划用 Fable 5.2，写代码用更便宜的 Opus 或 Sonnet。Claude Code 没有这项设置：opusplan 仍是先 Opus 再 Sonnet。可以先用 /model fable 做规划再切换，或给 subagent 单独指定模型。在 Coppice 里，每个智能体有自己的模型，PM 可以用 Fable 规划，工程师用 Opus 或 Sonnet 写代码。"
published: "2026-11-11"
draft: false
author: Coppice
authorType: Organization
keywords:
  - Fable
  - Opus
  - Sonnet
  - Claude Code
  - 规划
  - 写代码
---

## 难在哪里

Claude Fable 5.2 是 Anthropic 用来处理最难、最开放的工作的模型。对于多数写代码的情况，Anthropic 说先用 Opus 5.5，范围更小的任务再用 Sonnet 5.5。Fable 比 Opus 贵，Opus 比 Sonnet 贵。大部分 token 花在计划之后：读文件、修改、重试。如果规划完还开着 Fable，它就会去做这些机械的工作。你要的是计划用 Fable、写代码用 Opus 或 Sonnet，而且开始写代码时计划还在。

## 做法一：先用 Fable 做规划，再切换

Claude Code 没有一项设置，能让计划用 Fable、写代码用更便宜的模型。`opusplan` 仍然是计划模式用 Opus，之后用 Sonnet。

先运行 `/model fable`，再进入计划模式。在终端里，这个别名是 Fable 5.2。在 Claude apps gateway 会话里，它仍是 Fable 5，所以那里要运行 `/model claude-fable-5-2`。计划写好后，运行 `/model opus` 或 `/model sonnet`。对话还在。会话一结束，计划就没了。

## 做法二：给 subagent 单独指定模型

Claude Code 的 subagent 是一个 Markdown 文件。它的 `model` 字段决定这个 subagent 用哪个模型：`fable`、`opus`、`sonnet`、`haiku`，或完整的模型 id。会话保持在 Fable，把写代码交给 Opus 或 Sonnet。把下面的文件放在 `.claude/agents/`：

```markdown
---
name: implementer
description: Implements an approved plan
model: opus
---
```

任务很小时用 `sonnet`。只有委派出去的工作跑在那个模型上。主会话自己写的代码仍跑在 Fable 上。

## 做法三：把计划贴进更便宜的会话

用 Fable 做规划，复制计划，再用 Opus 或 Sonnet 重新开一个会话：

```bash
claude --model opus
```

任务更小时，同样的一步是 `claude --model sonnet`。把计划贴进去。也可以用同样的办法贴进另一个智能体 CLI。新会话只知道你贴进去的内容。

## 做法四：用 Coppice，每个智能体一个模型

[Coppice](/) 是一个桌面应用，把你的编程智能体放在一块任务看板上。每个智能体有自己的模型，即使用的是同一个 CLI。像 Settings 里的深色模式开关这样的工单，PM 可以用 Fable 5.2 做规划，Frontend Engineer 可以用 Opus 5.5 或 Sonnet 来做。

1. [安装 Coppice](/docs/install)。在终端里按你平时的方式登录 Claude Code，再添加你的项目。
2. 创建一个 PM 智能体，选择 Claude Code，把模型设为 Fable 5.2。
3. 在 Claude Code 上创建 Technical Lead 和 Frontend Engineer。Frontend Engineer 的模型设为 Opus 5.5；改动很小时设为 Sonnet。
4. 写好工单，点击 **Run agent**。PM 用 Fable 写计划。读完后点击 **Approve plan** 或 **Ask for changes**。这次规划运行不会改你的代码。
5. 代码准备好之后，审核 diff。

你批准之后，Technical Lead 会检查计划，并指派 Frontend Engineer。Frontend Engineer 的运行使用你设的模型，而且会自己开始。[核心概念](/docs/concepts)说明了每一步。

![新建智能体对话框，名称是 PM。模型列表已打开，选中的是 Claude Fable 5.2。](/blog/fable-for-planning/pm-fable.png "PM 的模型列表，选中的是 Claude Fable 5.2。")

![Frontend Engineer 的新建智能体对话框。已选择 Claude Code，并标为 Ready，模型是 Claude Opus 5.5。](/blog/fable-for-planning/frontend-opus.png "Frontend Engineer 使用 Claude Code，模型选的是 Claude Opus 5.5。")

## 该选哪一种

- **只有一个 Claude Code 会话，而且计划要用 Fable：** 先 `/model fable`，再 `/model opus` 或 `/model sonnet`。Opus 和 Sonnet 就够用时，用 `opusplan`。
- **某一步要委派给更便宜的模型：** 建一个 subagent，`model` 设为 `opus` 或 `sonnet`。
- **想把计划放进一个新会话：** 贴进 Opus 或 Sonnet 的会话。
- **工单很多，规划用 Fable，写代码用 Opus 或 Sonnet：** 用 Coppice。从[快速上手](/docs/getting-started)开始。
