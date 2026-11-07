---
title: 核心概念
metaTitle: "核心概念：工单、智能体、聊天与审核"
description: "Coppice 的工单如何从 Backlog 走到 Done，每个智能体做什么，聊天和知识库如何工作，以及你在哪里审核并接受 diff。"
---

# 核心概念

Coppice 是一块用于编码工作的看板。你写工单，智能体在各自的分支上干活，每个改动在落地前都要经你审核。你启动工单之后，智能体可以按工作流自动继续。你审核并接受最终的 diff。你不决定，什么都不会合并。

<a id="how-a-ticket-moves"></a>

## 工单如何流转

1. **Backlog**（待办）。你创建一个工单，或批准一个智能体建议的工单。
2. **Plan Review**（计划审核）。你点击 **Run agent** 后，PM 智能体写计划。读完可以批准，也可以评论。工单如果有不清楚的地方，PM 智能体可能会先问你最多 3 个简短问题。卡片上会显示 "Waiting for your answers"（等待你的回答）。这时你在工单里回答并点击 **Send answers**，PM 智能体再写计划。规划永远不会改动工单的代码。开启 **Skip planning**（跳过规划）后，工单从 Backlog 直接进入 Ready。计划涉及流程、架构或数据模型时，里面可能附一张小图；文字版计划始终都在。
3. **Ready**（就绪）。你批准的计划会成为工单描述的一部分，放在 **Approved plan** 一节。Technical Lead（技术负责人）智能体会对照代码检查计划，只在需要时修改这一节，然后决定由谁来做。被指派智能体的运行会自动开始。想自己启动运行，就在 Settings 里设置 `auto_start_runs = false`。如果没有 Technical Lead，或者没有合适的人选，工单会留在 Ready，保持未指派，并附一条评论。
4. **In Progress、In Review、In QA**（进行中、审查中、测试中）。智能体在工单自己的分支上构建、审查和测试改动。你的主检出目录不会被动到。
5. **Wait for Human Review**（等待人工审核）。你打开 diff，用评论给出反馈，满意后选择 **Accept**（接受）。如果之后智能体又加了提交，工单会回到 In Review，然后再回到你这里。
6. **Done**（完成）。接受不等于合并。准备好后选择 **Merge…**。你接受的内容就是合并的内容，一字不差。如果文件冲突，Coppice 不会动 main，而是把工单送回 In Progress，由同一个智能体解决冲突。合并前你需要再接受一次。

卡住的工单会进入 **Blocked**（受阻），并附上一条说明原因的评论。

<a id="agents"></a>

## 智能体

Coppice 运行你已经在用的智能体 CLI，例如 Claude Code、Codex、Cursor、OpenCode、Kilo Code、GitHub Copilot、Qwen Code、Kimi Code 和 Grok。每个智能体都有一个角色，比如 PM、Technical Lead 或 Backend Engineer，并运行在一个 CLI 上。参见[提供商](/docs/providers)。

每个智能体都可以用不同的模型，即使在同一个 CLI 上也一样。你的 PM 可以用 Claude Opus，Backend Engineer 则用 Claude Sonnet。

<a id="chat"></a>

## 聊天

打开 **Chat**（聊天），点击一个智能体，就可以聊你的代码、一个想法或某个工单。每个智能体都能看到你所有的仓库、看板、工单和 Knowledge（知识库）。智能体可以起草工单，或建议一条 Knowledge 笔记，但你批准之前，什么都不会保存。选择 **New chat** 可以另开一个聊天。

所有 CLI 都能聊天。Coppice 还没有确认 Claude Code、Codex 或 Kilo Code 能保持只读，所以这几个 CLI 的聊天会显示一条提示横幅。这些智能体仍然可以在你的电脑上执行命令。

<a id="everything-is-on-the-card"></a>

## 一切都在卡片上

评论、计划和智能体的运行记录都保存在工单上。智能体之间只通过工单评论沟通，所以事情如果发生过，你就能在工单上看到。智能体可以打开你附加到描述或评论里的文件。

<a id="knowledge"></a>

## Knowledge（知识库）

智能体可以为后续工单保存关于你项目的笔记。默认情况下，保留哪些由你批准。
