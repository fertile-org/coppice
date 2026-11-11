---
title: 怎樣用 Git worktree 並行執行多個 AI 程式設計代理
description: "幾個程式設計代理一起跑，又不會互相覆蓋：每個任務一個 git worktree、一小段指令碼，或一塊替你準備好的看板。"
answer: "每個代理有自己的 git worktree：同一個倉庫裏的獨立資料夾，各自一條分支，所以不會改同一批檔案。你可以自己用 git worktree add 建立，也可以收進一小段指令碼，或者用 Coppice 這樣的看板。看板上，每張工單一開始工作就有自己的分支和 worktree，所有執行都集中在一處。無論哪種做法，每條分支都由你審查，再合併。"
published: "2026-11-07"
updated: "2026-11-09"
author: Coppice
authorType: Organization
draft: false
keywords:
  - git worktree
  - 並行代理
  - 程式設計代理
  - Claude Code
---

## 為甚麼這件事難辦

兩個代理在同一個資料夾裏，會改同一批檔案，一方會弄壞另一方的工作。Git worktree 就是用來解決這個問題：每個代理有自己的資料夾和分支。

難處跟着換了地方。每個代理一個終端，你要記得哪個做完、哪個在等你，分支也要逐條合併。兩件任務碰到同一批檔案，合併時仍然可能衝突。

## 做法一：自己建立 worktree

每個任務建一個 worktree：

```bash
git worktree add ../my-project-login -b login
git worktree add ../my-project-search -b search
```

在每個資料夾開一個終端，在那裏啟動代理。任務做完，審查並合併那條分支，再用 `git worktree remove` 刪掉這個資料夾。

兩三個任務，這樣做就夠了。再多的話，切換終端、追蹤每件任務，本身就變成工作。

## 做法二：收進一小段指令碼

一個 shell 函式可以一步建立 worktree，並啟動代理：

```bash
agent_task() {
  dir="../$(basename "$PWD")-$1"
  git worktree add "$dir" -b "$1" && cd "$dir" && claude
}
```

在專案資料夾執行 `agent_task login`。把 `claude` 換成 `codex`，或其他代理 CLI。

準備時少打一些字。每個任務仍然是一個終端，追蹤和合併仍然由你做。

## 做法三：用 Coppice，一塊看板放全部任務

[Coppice](/) 是一套桌面軟件，把程式設計代理放在同一塊工作看板上。工作一開始，每張工單就有自己的分支和 worktree，所以主檢出目錄不會被動到。多張工單可以同時進行，看板會顯示每張走到哪一步。

1. [安裝 Coppice](/docs/install)，並加入你的專案。
2. 建立你的代理，每個選擇 Claude Code、Codex 或其他 CLI。
3. 每個任務寫一張工單，然後讓代理開始工作。
4. 批准每份計劃。準備好了，再審查各自的 diff。
5. 一次合併一張工單。如果合併有衝突，Coppice 會列出檔案，你可以叫一個代理去解決。

![一張工單的 Git actions 欄，顯示自己的分支 agent/TICKET-723e8f0a，以及 worktree 資料夾。](/blog/parallel-agents-worktrees/branch-and-worktree.png "每張工單各自有分支和 worktree。")

[核心概念](/docs/concepts)說明工單會經過的每一步。

## 把任務拆開，合併才乾淨

無論用哪種做法：

- 每件任務用自己的檔案。lockfile、schema，或共用模組這類共用檔案，歸一件任務，或歸後面的任務。
- 先合併一條分支，再把下一條更新到這個結果上，並檢查兩處改動放在一起是否仍然說得通。Git 可以乾淨地合併兩個檔案，即使內容互相矛盾。

## 選哪一種

- **兩三個任務：** 自己建立 worktree。
- **你經常開新任務，而且喜歡終端：** 一小段指令碼。
- **同時有幾件任務，而且你想在一處查看、審查和合併：** 用 Coppice。由[快速上手](/docs/getting-started)開始。
