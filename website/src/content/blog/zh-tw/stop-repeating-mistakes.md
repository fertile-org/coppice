---
title: 如何讓 AI 程式設計代理不再重複同樣的錯誤
description: "把糾正放進規則檔案、會失敗的檢查、這次任務，或一個 skill。在 Coppice 裡批准一條 Knowledge 筆記，之後用詞相符的工單會列出這條筆記。"
answer: "把糾正寫在下一次工作階段能看見的地方。CLAUDE.md、AGENTS.md 或 Cursor 規則會載入之後的工作階段。測試或 lint 檢查會在錯誤再次出現時失敗。你也可以寫進這次任務，或放進代理在任務吻合時載入的 skill。在 Coppice 裡，把它存成 Knowledge 筆記並批准。之後若工單的用詞與筆記相符，代理會被告知去讀這條筆記。"
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

## 為什麼難

你糾正了代理。下一件任務從空白開始，同樣的錯誤又出現。聊天結束後，程式設計代理不會留住那次糾正。必須有東西把它放到下一次執行面前，或者在錯誤回來時讓一項檢查失敗。

## 做法一：專案裡的規則檔案

每次工作階段開始時，Claude Code 會讀 `CLAUDE.md`，從你啟動時所在的資料夾，以及它上面的資料夾。這條路徑上既沒有 `CLAUDE.md` 也沒有 `CLAUDE.local.md` 時，它改為讀 `AGENTS.md`。它還會載入自動記憶檔案 `MEMORY.md` 的前 200 行，或 25KB，以先到者為準。Claude Code 把這些檔案當作上下文。它們不會攔住一次編輯。

Codex 動手之前會讀 `AGENTS.md`。它從專案根目錄往下走到你啟動時所在的資料夾，合併後的檔案達到 32 KiB 就停。

Cursor 會讀專案根目錄裡的 `AGENTS.md`，以及你正在處理的檔案所在子目錄裡的 `AGENTS.md`。`.cursor/rules` 裡的專案規則，在你把它設為 always apply 時，會出現在每一次聊天裡。否則，Cursor 在檔案符合時、代理判斷它適用時，或你提到它時，才會把它加進來。規則檔案是建議。代理可以跳過其中一行。

## 做法二：測試或 lint 檢查

檢查能看出這個錯誤時，就加上這項檢查。一項測試在 Settings 控制項自己儲存主題狀態時失敗，或一條針對這種寫法的 lint 規則，會在下一次執行時攔住這次改動。

這能攔住這個錯誤。版面上的選擇仍然要人來看，因為檢查看不見它。

## 做法三：寫進任務，或放進 skill

把糾正貼進這次任務。代理這次執行能看見。下一件任務還得再貼一次。

skill 是一份簡短的說明檔案。任務吻合時，或你呼叫它時，代理才會載入。在 Claude Code 裡，在那之前這份檔案不進入工作階段。所以一行規則放在 `CLAUDE.md`，一套步驟放在 skill。代理仍然可以跳過它。

## 做法四：用 Coppice，批准一條 Knowledge 筆記

[Coppice](/) 是一個桌面應用程式，把程式設計代理放在一塊工作看板上。Knowledge（知識庫）是給以後工單用的專案筆記。筆記由你來寫；做工單時，代理也可以為以後工單需要的一條規則提出一條筆記。你批准之前，它一直待批准。之後的執行看不到待批准的筆記。

一條筆記可以涵蓋所有看板、一塊看板，或一塊看板上的一個代理。代理從工單提出的筆記，涵蓋那塊看板。

1. [安裝 Coppice](/docs/install)，並加入你的專案。
2. 開啟 Knowledge，加上一條筆記。若是 Settings 裡的深色模式開關，寫明控制項沿用現有的 theme hook，並且這次改動包含一項設定測試。選這項工作所在的看板。
3. 批准這條筆記。
4. 在下一張工單上，讓代理開始做。Coppice 在已批准的筆記裡搜尋工單標題和描述中的詞。符合的筆記會在執行開始時列給代理，並告訴它先讀這條筆記，再依賴它。

之後一張標題為 Add a dark mode toggle to Settings 的工單與這些詞相同，所以這條筆記會被列出。全文留在提示之外，直到代理去讀。代理也可以搜尋已批准的筆記。Coppice 以外的終端永遠看不到這條筆記。[核心概念](/docs/concepts#knowledge)說明了 Knowledge。

![新增 Knowledge 的對話框。標題是 Settings theme controls use the existing theme hook，類型是 Review feedback，看板是 acme-web，筆記寫明深色模式開關沿用現有的 theme hook，並包含一項設定測試。](/blog/stop-repeating-mistakes/add-note.png "筆記點出 theme hook，放在 acme-web 看板上。")

![acme-web 看板上一條待批准的 Knowledge 筆記。標題是 Settings theme controls use the existing theme hook，卡片上有 Approve。](/blog/stop-repeating-mistakes/pending-note.png "你批准之前，筆記一直待批准。")

## 該選哪一種

- **這個錯誤能讓一項檢查失敗：** 加上測試或 lint 規則。
- **一個工具的每次工作階段都該看見一條短規則：** `CLAUDE.md`、`AGENTS.md`，或一條 Cursor 規則。
- **只有一件任務，或一套你不想放進每次工作階段的步驟：** 寫進任務，或放進 skill。
- **工單很多，而且你想先批准保留什麼，下一次執行才能用：** 用 Coppice。從[快速上手](/docs/getting-started)開始。
