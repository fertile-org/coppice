---
title: 如何用 Git worktree 平行執行多個 AI 程式設計代理
description: "同時執行多個程式設計代理，又不讓它們互相覆寫：每個任務一個 git worktree、一段小指令碼，或一塊替你建好的看板。"
answer: "給每個代理各自的 git worktree：同一個倉庫的獨立資料夾，各自一條分支，這樣它們就不會改同一批檔案。你可以手動用 git worktree add 建立，也可以包進一段小指令碼，或者用 Coppice 這樣的看板。看板上，每張工單開始工作時都會有自己的分支和 worktree，所有執行都在一處可見。無論哪種做法，每條分支都由你審查，再合併。"
published: "2026-11-07"
updated: "2026-11-09"
author: Coppice
authorType: Organization
draft: false
keywords:
  - git worktree
  - 平行代理
  - 程式設計代理
  - Claude Code
---

## 為什麼難

兩個代理在同一個資料夾裡會改同一批檔案，一個會弄壞另一個的工作。Git worktree 解決的就是這件事：每個代理有自己的資料夾和分支。

接著，難的地方換了。每個代理一個終端，你得記住哪個做完了、哪個在等你，還得一條一條合併分支。兩件任務如果碰到同一批檔案，合併時仍然可能衝突。

## 做法一：手動建立 worktree

每個任務建一個 worktree：

```bash
git worktree add ../my-project-login -b login
git worktree add ../my-project-search -b search
```

在每個資料夾裡開啟一個終端，並在那裡啟動代理。任務做完後，審查並合併它的分支，再用 `git worktree remove` 刪掉這個資料夾。

兩三個任務時，這樣做很好。再多，切換終端、追蹤每件任務，本身就成了工作。

## 做法二：包進一段小指令碼

一個 shell 函式可以一步建立 worktree 並啟動代理：

```bash
agent_task() {
  dir="../$(basename "$PWD")-$1"
  git worktree add "$dir" -b "$1" && cd "$dir" && claude
}
```

在專案資料夾裡執行 `agent_task login`。把 `claude` 換成 `codex`，或其他代理 CLI。

這樣省掉了準備時的輸入。每個任務仍然是一個終端，追蹤和合併還是你來做。

## 做法三：用 Coppice，一塊看板放下所有任務

[Coppice](/) 是一個桌面應用程式，把程式設計代理放在一塊工作看板上。工作開始時，每張工單都有自己的分支和 worktree，所以你的主檢出目錄不會被改動。多張工單可以同時進行，看板顯示每張走到了哪一步。

1. [安裝 Coppice](/docs/install)，並加入你的專案。
2. 建立你的代理，並為每個代理選擇 Claude Code、Codex 或其他 CLI。
3. 每個任務寫一張工單，然後讓代理開始做。
4. 批准每份計劃，準備好之後再審查各自的 diff。
5. 一次合併一張工單。如果合併衝突，Coppice 會列出檔案，你可以讓一個代理去解決。

![一張工單的 Git actions 區域，顯示它自己的分支 agent/TICKET-723e8f0a，以及它的 worktree 資料夾。](/blog/parallel-agents-worktrees/branch-and-worktree.png "每張工單都有自己的分支和 worktree。")

[核心概念](/docs/concepts)說明了工單經過的每一步。

## 拆開任務，合併才乾淨

無論用哪種做法：

- 每件任務用自己的檔案。lockfile、schema，或共用模組這類共用檔案，歸到一件任務，或歸到後面的任務。
- 先合併一條分支，再把下一條更新到這個結果上，並檢查兩處改動放在一起是否仍然合理。Git 可以乾淨地合併兩個檔案，即使它們的內容互相矛盾。

## 該選哪一種

- **兩三個任務：** 手動建立 worktree。
- **你經常開始任務，而且喜歡終端：** 一段小指令碼。
- **同時有好幾件任務，而且你想在一處查看、審查和合併：** 用 Coppice。從[快速上手](/docs/getting-started)開始。
