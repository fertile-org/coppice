---
title: 核心概念
description: "工單如何從 Backlog 走到 Done，以及各自由誰負責。"
---

# 核心概念

Coppice 是一塊用於編碼工作的看板。你寫工單，代理在各自的分支上幹活，每個改動在落地前都要經你審查。你不動手，什麼都不會開始；你不決定，什麼都不會合併。

<a id="how-a-ticket-moves"></a>

## 工單如何流轉

1. **Backlog**（待辦）。你建立一個工單，或批准一個代理建議的工單。
2. **Plan Review**（計劃審查）。點選 **Run agent**，你的 PM 代理就會寫計劃。閲讀後，批准它或留下評論。如果工單有不清楚的地方，PM 代理可能會先問你最多 3 個簡短的問題。卡片上會顯示 "Waiting for your answers"（等待你的回答）。在工單裏回答後點選 **Send answers**，PM 代理就會寫計劃。規劃永遠不會改動工單的程式碼。開啓 **Skip planning**（跳過規劃）後，工單會從 Backlog 直接進入 Ready。涉及流程、架構或資料模型時，計劃裏可能附一張小圖，但文字版計劃始終都在。
3. **Ready**（就緒）。你批准計劃後，Technical Lead（技術負責人）代理會決定由誰來做。如果沒有 Technical Lead，或者沒有合適的人選，工單會留在 Ready，保持未指派，並附上一條評論。
4. **In Progress、In Review、In QA**（進行中、審查中、測試中）。代理在工單自己的分支上構建、審查和測試改動。你的主檢出目錄不會被動到。
5. **Wait for Human Review**（等待人工審查）。你開啓 diff，以評論的形式給出回饋，滿意後選擇 **Accept**（接受）。如果之後代理又加了提交，工單會回到 In Review，然後再回到你這裏。
6. **Done**（完成）。接受不等於合併。準備好後選擇 **Merge…**，你接受的內容就是合併的內容，一字不差。如果檔案衝突，Coppice 不會動 main，而是把工單送回 In Progress，由同一個代理解決衝突。合併前你需要再接受一次。

卡住的工單會進入 **Blocked**（受阻），並附上一條説明原因的評論。

<a id="agents"></a>

## 代理

Coppice 執行你已經在用的代理 CLI，例如 Claude Code、Codex、Cursor、OpenCode 和 Kilo Code。每個代理都有一個角色，比如 PM、Technical Lead 或 Backend Engineer，並執行在一個 CLI 上。參見[提供商](/docs/providers)。

每個代理都可以用不同的模型，即使在同一個 CLI 上也一樣。你的 PM 可以用 Claude Opus，Backend Engineer 則用 Claude Sonnet。

<a id="chat"></a>

## 聊天

開啓 **Chat**（聊天），點選一個代理，就可以聊你的程式碼、一個想法或某個工單。每個代理都能看到你所有的倉庫、看板、工單和 Knowledge（知識庫）。它可以起草工單或建議一條 Knowledge 筆記，但在你批准之前什麼都不會儲存。選擇 **New chat** 可以另開一個聊天。

所有 CLI 都能聊天。Coppice 還沒有確認 Claude Code、Codex、Cursor 或 Kilo Code 能保持只讀，所以它們的聊天會顯示一條提示橫幅。代理仍然可以在你的電腦上執行命令。

<a id="everything-is-on-the-card"></a>

## 一切都在卡片上

評論、計劃和代理的執行記錄都儲存在工單上。代理之間只通過工單評論溝通，所以發生過的事，你都能在那裏看到。

<a id="knowledge"></a>

## Knowledge（知識庫）

代理可以為後續工單儲存關於你專案的筆記。預設情況下，保留哪些由你批准。
