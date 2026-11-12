---
title: 怎樣避免 AI 程式設計代理重複同樣的錯誤
description: "把糾正放進規則檔案、會失敗的檢查、這一次任務，或一個 skill。在 Coppice 裏批准一條 Knowledge 筆記，之後用詞對得上的工單會列出這條筆記。"
answer: "把糾正寫在下一次工作階段看得到的地方。CLAUDE.md、AGENTS.md 或 Cursor 規則會載入之後的工作階段。測試或 lint 檢查會在錯誤再出現時失敗。你也可以寫進這一次任務，或放進代理在任務吻合時載入的 skill。在 Coppice 裏，把它存成 Knowledge 筆記並批准。之後若工單的用詞對得上筆記，代理會被告知去讀這條筆記。"
published: "2026-11-11"
author: Coppice
authorType: Organization
draft: false
keywords:
  - AI 程式設計代理
  - CLAUDE.md
  - AGENTS.md
  - Cursor 規則
  - Knowledge
---

## 為甚麼這件事難辦

你已經糾正過代理。下一件任務卻從空白開始，同一個錯誤又出現。聊天一結束，程式設計代理就不會留下那次糾正。要麼有東西在下一次執行前把它擺到面前，要麼錯誤回來時有一項檢查失敗。

## 做法一：專案裏的規則檔案

每次工作階段開始，Claude Code 都會讀 `CLAUDE.md`，從你啟動的資料夾，以及它上面的資料夾。這條路徑既沒有 `CLAUDE.md` 也沒有 `CLAUDE.local.md` 時，它改讀 `AGENTS.md`。它還會讀入自動記憶檔案 `MEMORY.md` 的前 200 行，或 25KB，以先到的那個為準。Claude Code 把這些檔案當上下文。它們不會阻止一次編輯。

Codex 動手前會讀 `AGENTS.md`。它從專案根目錄向下走到你啟動的資料夾，合併後的檔案一到 32 KiB 就停。

Cursor 會讀專案根目錄的 `AGENTS.md`，也會讀你正在處理的檔案所在子目錄裏的 `AGENTS.md`。`.cursor/rules` 裏的專案規則，設成 always apply 之後，每次聊天都會有。否則，要檔案符合、代理判斷它適用，或者你提到它，Cursor 才會加進來。規則檔案只是建議。代理可以跳過其中一行。

## 做法二：測試或 lint 檢查

檢查看得到這個錯誤，就加上這項檢查。Settings 的控制自己儲存主題狀態時會失敗的測試，或針對這種寫法的 lint 規則，會在下一次執行攔住這次改動。

這樣能攔住這個錯誤。版面怎樣排仍然要人看，因為檢查看不見。

## 做法三：寫進任務，或者放進 skill

把糾正貼進這一次任務。代理這次執行看得到。下一件任務就要再貼一次。

skill 是一份短的說明檔案。任務吻合，或者你叫它出來，代理才載入。在 Claude Code 裏，在那之前這份檔案不進工作階段。所以一行規則放 `CLAUDE.md`，一套步驟放 skill。代理仍然可以跳過。

## 做法四：用 Coppice，批准一條 Knowledge 筆記

[Coppice](/) 是一套桌面軟件，把程式設計代理放在同一塊工作看板上。Knowledge（知識庫）是留給以後工單的專案筆記。筆記可以由你寫；做工單的時候，代理也可以為以後工單需要的一條規則提出筆記。你批准之前，它一直待批准。之後的執行看不到待批准的筆記。

一條筆記可以涵蓋所有看板、一塊看板，或一塊看板上的一個代理。代理從工單提出的筆記，涵蓋的是那塊看板。

1. [安裝 Coppice](/docs/install)，並加入你的專案。
2. 打開 Knowledge，加一條筆記。若是 Settings 裏的深色模式開關，寫明控制沿用現有的 theme hook，而且這次改動包含一項設定測試。選這項工作所在的看板。
3. 批准這條筆記。
4. 下一張工單，讓代理開始工作。Coppice 在已批准的筆記裏搜尋工單標題和描述的用詞。對得上的筆記會在執行開始時列給代理，並叫它先讀這條筆記，再依賴它。

之後一張標題是 Add a dark mode toggle to Settings 的工單共用這些詞，所以這條筆記會被列出。全文留在提示外面，直到代理去讀。代理也可以搜尋已批准的筆記。Coppice 以外的終端永遠看不到這條筆記。[核心概念](/docs/concepts#knowledge)說明 Knowledge。

![加入 Knowledge 的對話框。標題是 Settings theme controls use the existing theme hook，類型是 Review feedback，看板是 acme-web。筆記寫明深色模式開關沿用現有的 theme hook，並包含一項設定測試。](/blog/stop-repeating-mistakes/add-note.png "筆記點出 theme hook，放在 acme-web 看板上。")

![acme-web 看板上，一條待批准的 Knowledge 筆記。標題是 Settings theme controls use the existing theme hook，卡片上有 Approve。](/blog/stop-repeating-mistakes/pending-note.png "你批准之前，筆記一直待批准。")

## 選哪一種

- **這個錯誤可以令一項檢查失敗：** 加測試或 lint 規則。
- **一個工具的每次工作階段都該看到一條短規則：** `CLAUDE.md`、`AGENTS.md`，或一條 Cursor 規則。
- **只有一件任務，或一套你不想放進每次工作階段的步驟：** 寫進任務，或放進 skill。
- **工單多，而且你想先批准留甚麼，下一次執行才能用：** 用 Coppice。由[快速上手](/docs/getting-started)開始。
