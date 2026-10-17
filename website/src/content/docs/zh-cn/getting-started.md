---
title: 快速上手
description: "从全新安装到第一次运行智能体，只需五步。"
---

# 快速上手

从全新安装到第一次运行智能体，只需五步。你需要一个本地 git 仓库和一个智能体 CLI，不需要 Coppice 账号。

全新安装后，Coppice 会打开一个简短的设置向导，帮你添加第一个仓库、看板和初始团队。之后可以在 Tools 中重新打开它。

<a id="1-install-a-provider-and-sign-in"></a>

## 1. 安装提供商并登录
提供商就是 Coppice 替你运行的智能体 CLI：Claude Code、Codex、Cursor、OpenCode 或 Kilo Code。Coppice 不负责安装它们，而是查找你电脑上已有的 CLI。

1. 像平常一样安装 CLI，并在终端里登录。
2. 在 Coppice 侧边栏中打开 **Tools**（工具），再打开 **Connectors**（连接器）标签页。
3. 找到你的 CLI 卡片。Coppice 可以使用它时，卡片会显示 **Ready**（就绪）。

如果显示 **Found, not signed in**（已找到，未登录），请在终端里登录该 CLI。如果显示 **Not on your PATH**（不在 PATH 中），请安装它，或按卡片上的 **Install docs**（安装文档）操作。然后点击 **Run check**（运行检查）。

Kilo Code 显示 **Installed**（已安装）。不消耗你的套餐额度，Coppice 就无法检查它的登录状态，所以如果你已退出登录，运行时会报错。

<a id="2-set-up-an-agent"></a>

## 2. 设置智能体
1. 在侧边栏中打开 **Agents**（智能体），点击 **New agent**（新建智能体）。
2. 选择一个 **Preset**（预设）。它会填好角色和提示词。
3. 输入 **Name**（名称）。
4. 选择 **Connector**（连接器）。Coppice 会默认选中第一个就绪的连接器。
5. 选择 **Model**（模型），或保留默认值。
6. 点击 **Create agent**。

保存时会启用该连接器，你会看到 "{name} turned on."（已启用 {name}）。

请添加一个 **PM** 智能体、一个 **Technical Lead**（技术负责人）智能体，以及至少一个干活的智能体（**Frontend Engineer**、**Backend Engineer** 或 **Research**）。规划和交接需要这三类角色都在。

<figure>
<img src="/assets/docs/new-agent.webp" width="1440" height="900" alt="新建智能体表单，已填写名称并选好角色和 CLI。">
<figcaption>每个智能体都运行在你的某个 CLI 上。</figcaption>
</figure>

<a id="3-add-a-repository"></a>

## 3. 添加仓库
1. 在侧边栏中打开 **Repositories**（仓库），点击 **Add repository**（添加仓库）。
2. 在 **Local path**（本地路径）旁点击 **Browse…**，选择一个本地 git 仓库。
3. Coppice 会尽量根据仓库自动填写 **Name**、**Remote URL (optional)** 和 **Default branch**。请检查一下。
4. 点击 **Add repository**。

新的一行应显示 **Ready**。如果显示 **Path missing**（路径不存在）或 **Not a git repo**（不是 git 仓库），请重新选择文件夹。

<a id="4-set-up-a-board"></a>

## 4. 设置看板
1. 在侧边栏中打开 **Boards**（看板）。
2. 点击 **New board**。如果是全新安装，点击 **Create board**。
3. 输入 **Name**，然后点击 **Create board**。

Coppice 会打开你的新看板。

<a id="5-create-a-ticket-and-run-it"></a>

## 5. 创建并运行工单
1. 在 **Backlog** 列的 **Add ticket…** 中输入标题，按 Enter。
2. 打开这张卡片。
3. 点击 **Edit** 添加描述，然后点击 **Save**。
4. 在 **Detail**（详情）中，把 **Assignee**（负责人）设为你的智能体，把 **Repository** 设为你的仓库。点击 **Save metadata**。
5. 点击 **Run agent**（运行智能体）。

工单会切换到 **Live Console**（实时控制台），你可以看着智能体工作。点击 **Stop** 可结束运行。

在工单有负责人、且所选仓库显示 **Ready** 之前，**Run agent** 按钮不可用。仅仅指派智能体并不会自动开始运行。

<figure>
<img src="/assets/docs/backlog-ticket.webp" width="1440" height="900" alt="看板的 Backlog 中有一个新工单，并显示 Run agent 按钮。">
<figcaption>点击 Run agent，PM 就会写计划。</figcaption>
</figure>

<a id="what-happens-next"></a>

## 接下来会发生什么
1. **Plan Review**（计划审核）。PM 智能体写好计划后，工单会移到 Plan Review，等你批准计划。点击 **Approve plan**（批准计划）。
2. **Ready**。你批准后，工单移到 Ready，由智能体决定接下来谁来做。如果没有合适的人选，它们会让工单保持未指派，并留言说明原因。
3. **In Progress**。被指派的智能体在自己的 git worktree 中完成工作。
4. **Wait for Human Review**（等待人工审核）。你来阅读 diff，在 Human Review 中点击 **Accept**（接受）。
5. **Done**。Done 卡片上可能显示 "Accepted, not merged"（已接受，未合并）。打开工单选择 **Merge…**；如果你更想以 pull request 的形式审核，就选 **Create PR via API**。

想跳过计划，就在工单上勾选 **Skip planning**（跳过规划）。工单会从 Backlog 直接进入 Ready。
