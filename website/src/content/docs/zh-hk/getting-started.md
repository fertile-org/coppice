---
title: 快速上手
description: "從全新安裝到第一次執行代理，只需五步。"
---

# 快速上手

從全新安裝到第一次執行代理，只需五步。你需要一個本地 git 倉庫和一個代理 CLI，不需要 Coppice 帳號。

全新安裝後，Coppice 會開啓一個簡短的設定嚮導，幫你新增第一個倉庫、看板和初始團隊。之後可以在 **System → Connectors** 中點選 **Set up workspace** 重新開啓它。

<a id="1-install-a-provider-and-sign-in"></a>

## 1. 安裝提供商並登入
提供商就是 Coppice 替你執行的代理 CLI：Claude Code、Codex、Cursor、OpenCode、Kilo Code、GitHub Copilot、Qwen Code、Kimi Code 或 Grok。Coppice 不負責安裝它們，而是查詢你電腦上已有的 CLI。

1. 像平常一樣安裝 CLI，並在終端裏登入。
2. 在 Coppice 側邊欄中開啓 **System**（系統），再開啓 **Connectors**（聯結器）。
3. 找到你的 CLI 卡片。Coppice 可以使用它時，卡片會顯示 **Ready**（就緒）。

如果顯示 **Found, not signed in**（已找到，未登入），請在終端裏登入該 CLI。如果顯示 **Not on your PATH**（不在 PATH 中），請安裝它，或按卡片上的 **Install docs**（安裝文件）操作。然後點選 **Run check**（執行檢查）。

Kilo Code、GitHub Copilot、Qwen Code 和 Kimi Code 顯示 **Installed**（已安裝）。不消耗你的套餐額度，Coppice 就無法檢查它們的登入狀態，所以如果你已退出登入，執行時會報錯。

<a id="2-set-up-an-agent"></a>

## 2. 設定代理
1. 在側邊欄中開啓 **Agents**（代理），點選 **New agent**（新建代理）。
2. 選擇一個 **Preset**（預設）。它會填好角色和提示詞。
3. 輸入 **Name**（名稱）。
4. 選擇 **Connector**（聯結器）。Coppice 會預設選中第一個就緒的聯結器。
5. 選擇 **Model**（模型），或保留預設值。
6. 點選 **Create agent**。

儲存時會啓用該聯結器，你會看到 "{name} turned on."（已啓用 {name}）。

請新增一個 **PM** 代理、一個 **Technical Lead**（技術負責人）代理，以及至少一個幹活的代理（**Frontend Engineer**、**Backend Engineer** 或 **Research**）。規劃和交接需要這三類角色都在。

<figure>
<img src="/assets/docs/new-agent.webp" width="1440" height="900" alt="新建代理表單，已填寫名稱並選好角色和 CLI。">
<figcaption>每個代理都執行在你的某個 CLI 上。</figcaption>
</figure>

<a id="3-add-a-repository"></a>

## 3. 新增倉庫
1. 在側邊欄中開啓 **Repositories**（倉庫），點選 **Add repository**（新增倉庫）。
2. 在 **Local path**（本地路徑）旁點選 **Browse…**，選擇一個本地 git 倉庫。
3. Coppice 會盡量根據倉庫自動填寫 **Name**、**Remote URL (optional)** 和 **Default branch**。請檢查一下。
4. 點選 **Add repository**。

新的一行應顯示 **Ready**。如果顯示 **Path missing**（路徑不存在）或 **Not a git repo**（不是 git 倉庫），請重新選擇資料夾。

<a id="4-set-up-a-board"></a>

## 4. 設定看板
1. 在側邊欄中開啓 **Boards**（看板）。
2. 點選 **New board**。如果是全新安裝，點選 **Create board**。
3. 輸入 **Name**，然後點選 **Create board**。

Coppice 會開啓你的新看板。

<a id="5-create-a-ticket-and-run-it"></a>

## 5. 建立並執行工單
1. 在 **Backlog** 列的 **Add ticket…** 中輸入標題，按 Enter。
2. 開啓這張卡片。
3. 點選 **Edit** 新增描述，然後點選 **Save**。
4. 在 **Detail**（詳情）中，把 **Assignee**（負責人）設為你的代理，把 **Repository** 設為你的倉庫。點選 **Save metadata**。
5. 點選 **Run agent**（執行代理）。

工單會切換到 **Live Console**（即時控制台），你可以看着代理工作。點選 **Stop** 可結束執行。

在工單有負責人、且所選倉庫顯示 **Ready** 之前，**Run agent** 按鈕不可用。僅僅指派代理並不會自動開始執行。

<figure>
<img src="/assets/docs/backlog-ticket.webp" width="1440" height="900" alt="看板的 Backlog 中有一個新工單，並顯示 Run agent 按鈕。">
<figcaption>點選 Run agent，PM 就會寫計劃。</figcaption>
</figure>

<a id="what-happens-next"></a>

## 接下來會發生什麼
1. **Plan Review**（計劃審查）。PM 代理寫好計劃後，工單會移到 Plan Review，等你批准計劃。點選 **Approve plan**（批准計劃）。
2. **Ready**。你批准後，工單移到 Ready，由代理決定接下來誰來做。如果沒有合適的人選，它們會讓工單保持未指派，並留言説明原因。
3. **In Progress**。被指派的代理在自己的 git worktree 中完成工作。
4. **Wait for Human Review**（等待人工審查）。你來閲讀 diff，在 Human Review 中點選 **Accept**（接受）。
5. **Done**。Done 卡片上可能顯示 "Accepted, not merged"（已接受，未合併）。開啓工單選擇 **Merge…**；如果你更想以 pull request 的形式審查，就選 **Create PR via API**。

想跳過計劃，就在工單上勾選 **Skip planning**（跳過規劃）。工單會從 Backlog 直接進入 Ready。
