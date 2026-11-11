---
title: 如何在同一個專案裏一起用 Claude Code 和 Codex
metaTitle: 同一個專案裏用 Claude Code 和 Codex
description: "在同一份程式碼上執行 Claude Code 和 Codex，有三種做法：兩個終端、git worktree，或一塊看板，每個代理做自己的工單。"
answer: "給每個工具一份自己的程式碼副本，它們就不會同時改同一批檔案。你可以自己用 git worktree 分開，也可以用 Coppice 這樣的看板：每張工單有自己的分支，由你決定這張工單用 Claude Code 還是 Codex。無論哪種，改動進入主分支之前都要你審查。"
published: "2026-11-07"
updated: "2026-11-08"
draft: false
author: Coppice
authorType: Organization
keywords:
  - Claude Code
  - Codex
  - 程式設計代理
  - git worktree
---

## 為甚麼要一起用

Claude Code 和 Codex 都是在終端裏跑的程式設計代理。你可以叫一個做計劃、另一個寫程式碼，也可以叫第二個模型去審查第一個改過的地方。難處不在於把它們跑起來，而在於把兩邊的改動分開，你才分得出是誰改了甚麼。

常見的做法有三種。

## 做法一：兩個終端，同一份檢出

用兩個終端開啟同一個專案。一邊執行 `claude`，另一邊執行 `codex`。

這是最快可以開始的做法。一次只用一個工具時很合適，例如用 Codex 審查 Claude Code 剛寫好的內容。兩個同時改的話，它們可能改到同一個檔案，後寫的一方可能覆蓋先寫的工作。

## 做法二：每個工具一個 git worktree

git worktree 是同一個倉庫的第二個工作資料夾，掛在自己的分支上。為 Codex 建立一個：

```bash
git worktree add ../my-project-codex -b codex-task
```

主資料夾裏執行 `claude`，`../my-project-codex` 裏執行 `codex`。兩邊可以同時工作，不會碰到對方的檔案。完成後，你自己審查每個分支，再自己合併。

工作因此分開，但協調仍然由你負責：哪件工作交給哪個工具、兩邊怎樣交接上下文，以及用完的分支怎樣清理。

## 做法三：用 Coppice，一塊看板同時放兩個

[Coppice](/) 是一套桌面軟件，把程式設計代理放在同一塊工作看板上。它執行的是你電腦上已經有的 `claude` 和 `codex` 命令。每張工單在自己的分支上執行，主檢出目錄不會被動到。代理透過看板把工單交給對方。

1. [安裝 Coppice](/docs/install)。在終端裏像平常一樣登入 Claude Code 和 Codex。
2. 把專案資料夾加入倉庫。
3. 建立代理，每個選擇 Claude Code 或 Codex。例如 PM 和 Technical Lead（技術負責人）用 Claude Code，Backend Engineer 用 Codex，Reviewer 用 Claude Code，讓 Claude Code 審查 Codex 寫的內容。
4. 寫好工單，讓代理開始工作。

PM 會寫出計劃，由你批准。之後代理會構建並審查這次改動。你讀最終的 diff 並接受，準備好了再合併。

![新建代理的畫面。已選取 PM 預設，代理的 CLI 是 Claude Code。](/blog/claude-code-and-codex/new-agent.png "每個代理只用一個 CLI。建立時選擇 Claude Code 或 Codex。")

## 選哪一種

- **一件很快做完的工作，一次只用一個工具：** 兩個終端。
- **要並行，而且你願意自己管理分支：** git worktree。
- **工單多，代理會互相交接並審查對方的工作，計劃和 diff 都由你批准：** 用 Coppice。由[快速上手](/docs/getting-started)開始。
