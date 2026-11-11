---
title: 如何用 Fable 做計劃，實作改用比較便宜的模型
metaTitle: 用 Fable 做計劃，實作用便宜模型
description: "計劃用 Fable 5.2，實作用 Opus 或 Sonnet。Claude Code 沒有先 Fable 再換便宜模型的設定。Coppice 可以為每個代理各自設定模型。"
answer: "計劃用 Fable 5.2，實作用比較便宜的 Opus 或 Sonnet。Claude Code 沒有這項設定：opusplan 仍是先 Opus 再 Sonnet。可以先用 /model fable 做計劃再切換，或讓 subagent 使用自己的模型。在 Coppice 裡，每個代理有自己的模型，PM 可以用 Fable 做計劃，工程師用 Opus 或 Sonnet 實作。"
published: "2026-11-11"
draft: false
author: Coppice
authorType: Organization
keywords:
  - Fable
  - Opus
  - Sonnet
  - Claude Code
  - 計劃
  - 實作
---

## 難在哪裡

Claude Fable 5.2 是 Anthropic 用來處理最難、最開放的工作的模型。對於多數寫程式的情況，Anthropic 說先用 Opus 5.5，範圍比較小的工作再用 Sonnet 5.5。Fable 比 Opus 貴，Opus 比 Sonnet 貴。大部分 token 花在計劃之後：讀取檔案、修改、重試。計劃做完還開著 Fable，它就會去做這些機械的工作。你要的是計劃用 Fable、實作用 Opus 或 Sonnet，而且開始實作時計劃還在。

## 做法一：先用 Fable 做計劃，再切換

Claude Code 沒有一項設定，能讓計劃用 Fable、實作用比較便宜的模型。`opusplan` 仍然是計劃模式用 Opus，之後用 Sonnet。

先執行 `/model fable`，再進入計劃模式。在終端裡，這個別名是 Fable 5.2。在 Claude apps gateway 的工作階段裡，同一個別名仍是 Fable 5，所以那裡要執行 `/model claude-fable-5-2`。計劃寫好後，執行 `/model opus` 或 `/model sonnet`。對話還在。工作階段一結束，計劃就沒了。

## 做法二：讓 subagent 使用自己的模型

Claude Code 的 subagent 是一個 Markdown 檔案。它的 `model` 欄位決定這個 subagent 用哪個模型：`fable`、`opus`、`sonnet`、`haiku`，或完整的模型 id。工作階段保持在 Fable，把實作交給 Opus 或 Sonnet。把下面的檔案放在 `.claude/agents/`：

```markdown
---
name: implementer
description: Implements an approved plan
model: opus
---
```

工作很小時用 `sonnet`。只有交出去的工作跑在那個模型上。主工作階段自己寫的程式碼仍跑在 Fable 上。

## 做法三：把計劃貼進比較便宜的工作階段

用 Fable 做計劃，複製計劃，再用 Opus 或 Sonnet 重新開一個工作階段：

```bash
claude --model opus
```

工作更小時，同樣的一步是 `claude --model sonnet`。把計劃貼進去。也可以用同樣的辦法貼進另一個代理 CLI。新的工作階段只知道你貼進去的內容。

## 做法四：用 Coppice，每個代理一個模型

[Coppice](/) 是一個桌面應用程式，把你的程式設計代理放在一塊工作看板上。每個代理有自己的模型，即使走同一個 CLI 也一樣。像 Settings 裡的深色模式開關這樣的工單，PM 可以用 Fable 5.2 做計劃，Frontend Engineer 可以用 Opus 5.5 或 Sonnet 來做。

1. [安裝 Coppice](/docs/install)。在終端裡按你平常的方式登入 Claude Code，再加入你的專案。
2. 建立一個 PM 代理，選擇 Claude Code，把模型設為 Fable 5.2。
3. 在 Claude Code 上建立 Technical Lead（技術負責人）和 Frontend Engineer。Frontend Engineer 的模型設為 Opus 5.5；改動很小時設為 Sonnet。
4. 寫好工單，讓代理開始做。PM 用 Fable 寫計劃。讀完後，批准計劃，或要求修改。這次規劃不會改你的程式碼。
5. 程式碼準備好之後，審查 diff。

你批准之後，Technical Lead 會檢查計劃，並指派 Frontend Engineer。Frontend Engineer 的執行使用你設定的模型，而且會自己開始。[核心概念](/docs/concepts)說明了每一步。

![新建代理的畫面，名稱是 PM。模型清單已開啟，選取的是 Claude Fable 5.2。](/blog/fable-for-planning/pm-fable.png "PM 的模型清單，選取的是 Claude Fable 5.2。")

![Frontend Engineer 的新建代理畫面。已選擇 Claude Code，並標為 Ready，模型是 Claude Opus 5.5。](/blog/fable-for-planning/frontend-opus.png "Frontend Engineer 使用 Claude Code，模型選的是 Claude Opus 5.5。")

## 該選哪一種

- **只有一個 Claude Code 工作階段，而且計劃要用 Fable：** 先 `/model fable`，再 `/model opus` 或 `/model sonnet`。Opus 和 Sonnet 就夠用時，用 `opusplan`。
- **某一步要交給比較便宜的模型：** 建一個 subagent，`model` 設為 `opus` 或 `sonnet`。
- **想把計劃放進一個新的工作階段：** 貼進 Opus 或 Sonnet 的工作階段。
- **工單很多，計劃用 Fable，實作用 Opus 或 Sonnet：** 用 Coppice。從[快速上手](/docs/getting-started)開始。
