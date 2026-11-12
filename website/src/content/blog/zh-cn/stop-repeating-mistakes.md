---
title: 如何让 AI 编程智能体不再重复同样的错误
description: "把纠正放进规则文件、会失败的检查、这次任务，或一个 skill。在 Coppice 里批准一条 Knowledge 笔记，之后用词相符的工单会列出这条笔记。"
answer: "把纠正写在下一次会话能看见的地方。CLAUDE.md、AGENTS.md 或 Cursor 规则会载入之后的会话。测试或 lint 检查会在错误再次出现时失败。你也可以写进这次任务，或放进智能体在任务吻合时加载的 skill。在 Coppice 里，把它存成 Knowledge 笔记并批准。之后若工单的用词与笔记相符，智能体会被告知去读这条笔记。"
published: "2026-11-11"
author: Coppice
authorType: Organization
draft: false
keywords:
  - AI 编程智能体
  - CLAUDE.md
  - AGENTS.md
  - Cursor 规则
  - Knowledge
---

## 为什么难

你纠正了智能体。下一件任务从空白开始，同样的错误又出现。聊天结束后，编程智能体不会留住那次纠正。必须有东西把它放到下一次运行面前，或者在错误回来时让一项检查失败。

## 做法一：项目里的规则文件

每次会话开始时，Claude Code 会读 `CLAUDE.md`，从你启动时所在的目录，以及它上面的目录。这条路径上既没有 `CLAUDE.md` 也没有 `CLAUDE.local.md` 时，它改为读 `AGENTS.md`。它还会载入自动记忆文件 `MEMORY.md` 的前 200 行，或 25KB，以先到者为准。Claude Code 把这些文件当作上下文。它们不会拦住一次编辑。

Codex 动手之前会读 `AGENTS.md`。它从项目根目录往下走到你启动时所在的目录，合并后的文件达到 32 KiB 就停。

Cursor 会读项目根目录里的 `AGENTS.md`，以及你正在处理的文件所在子目录里的 `AGENTS.md`。`.cursor/rules` 里的项目规则，在你把它设为 always apply 时，会出现在每一次聊天里。否则，Cursor 在文件匹配时、智能体判断它适用时，或你提到它时，才会把它加进来。规则文件是建议。智能体可以跳过其中一行。

## 做法二：测试或 lint 检查

检查能看出这个错误时，就加上这项检查。一项测试在 Settings 控件自己保存主题状态时失败，或一条针对这种写法的 lint 规则，会在下一次运行时拦住这次改动。

这能拦住这个错误。布局上的选择仍然要人来看，因为检查看不见它。

## 做法三：写进任务，或放进 skill

把纠正贴进这次任务。智能体这次运行能看见。下一件任务还得再贴一次。

skill 是一份简短的说明文件。任务吻合时，或你调用它时，智能体才会加载。在 Claude Code 里，在那之前这份文件不进入会话。所以一行规则放在 `CLAUDE.md`，一套步骤放在 skill。智能体仍然可以跳过它。

## 做法四：用 Coppice，批准一条 Knowledge 笔记

[Coppice](/) 是一个桌面应用，把编程智能体放在一块任务看板上。Knowledge（知识库）是给以后工单用的项目笔记。笔记由你来写；做工单时，智能体也可以为以后工单需要的一条规则提出一条笔记。你批准之前，它一直待批准。之后的运行看不到待批准的笔记。

一条笔记可以覆盖所有看板、一块看板，或一块看板上的一个智能体。智能体从工单提出的笔记，覆盖那块看板。

1. [安装 Coppice](/docs/install)，并加入你的项目。
2. 打开 Knowledge，加上一条笔记。若是 Settings 里的深色模式开关，写明控件沿用现有的 theme hook，并且这次改动包含一项设置测试。选这项工作所在的看板。
3. 批准这条笔记。
4. 在下一张工单上，让智能体开始做。Coppice 在已批准的笔记里搜索工单标题和描述中的词。匹配的笔记会在运行开始时列给智能体，并告诉它先读这条笔记，再依赖它。

之后一张标题为 Add a dark mode toggle to Settings 的工单与这些词相同，所以这条笔记会被列出。全文留在提示之外，直到智能体去读。智能体也可以搜索已批准的笔记。Coppice 以外的终端永远看不到这条笔记。[核心概念](/docs/concepts#knowledge)说明了 Knowledge。

![添加 Knowledge 的对话框。标题是 Settings theme controls use the existing theme hook，类型是 Review feedback，看板是 acme-web，笔记写明深色模式开关沿用现有的 theme hook，并包含一项设置测试。](/blog/stop-repeating-mistakes/add-note.png "笔记点出 theme hook，放在 acme-web 看板上。")

![acme-web 看板上一条待批准的 Knowledge 笔记。标题是 Settings theme controls use the existing theme hook，卡片上有 Approve。](/blog/stop-repeating-mistakes/pending-note.png "你批准之前，笔记一直待批准。")

## 该选哪一种

- **这个错误能让一项检查失败：** 加上测试或 lint 规则。
- **一个工具的每次会话都该看见一条短规则：** `CLAUDE.md`、`AGENTS.md`，或一条 Cursor 规则。
- **只有一件任务，或一套你不想放进每次会话的步骤：** 写进任务，或放进 skill。
- **工单很多，而且你想先批准保留什么，下一次运行才能用：** 用 Coppice。从[快速上手](/docs/getting-started)开始。
