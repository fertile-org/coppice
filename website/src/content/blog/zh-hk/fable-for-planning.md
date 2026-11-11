---
title: 如何用 Fable 做計劃，寫程式改用較便宜的模型
metaTitle: 用 Fable 做計劃，寫程式用較便宜模型
description: "計劃用 Fable 5.2，寫程式用 Opus 或 Sonnet。Claude Code 沒有先 Fable 再換較便宜模型的設定。Coppice 可以為每個代理各自設定模型。"
answer: "計劃用 Fable 5.2，寫程式用較便宜的 Opus 或 Sonnet。Claude Code 沒有這項設定：opusplan 仍是先 Opus 再 Sonnet。可以先用 /model fable 做計劃再轉換，或讓 subagent 用自己的模型。在 Coppice 裏，每個代理有自己的模型，PM 可以用 Fable 做計劃，工程師用 Opus 或 Sonnet 寫程式。"
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
  - 寫程式
---

## 為甚麼這件事難辦

Claude Fable 5.2 是 Anthropic 給最難、最沒有固定範圍的工作用的模型。寫程式時，Anthropic 說多數情況先從 Opus 5.5 開始；工作範圍較小，再用 Sonnet 5.5。Fable 比 Opus 貴，Opus 又比 Sonnet 貴。大部分 token 出現在計劃之後，用在讀檔案、修改和重試。計劃做好仍留在 Fable，這些機械的工作也會由 Fable 來做。你想要的是：計劃用 Fable，寫程式用 Opus 或 Sonnet，而且開始寫的時候計劃還在。

## 做法一：先用 Fable 做計劃，再換模型

Claude Code 沒有一項設定，可以讓計劃用 Fable、寫程式用較便宜的模型。`opusplan` 仍然是計劃模式用 Opus，之後才用 Sonnet。

先執行 `/model fable`，再進入計劃模式。在終端裏，這個別名是 Fable 5.2。在 Claude apps gateway 的工作階段裏，同一個別名仍然是 Fable 5，所以那裏要執行 `/model claude-fable-5-2`。計劃寫好之後，執行 `/model opus` 或 `/model sonnet`。對話會留下。工作階段結束，計劃就消失。

## 做法二：給 subagent 自己的模型

Claude Code 的 subagent 是一個 Markdown 檔案。`model` 欄位決定這個 subagent 用哪個模型，可以是 `fable`、`opus`、`sonnet`、`haiku`，或完整的模型 id。工作階段留在 Fable，把寫程式交給 Opus 或 Sonnet。把下面的檔案儲存到 `.claude/agents/`：

```markdown
---
name: implementer
description: Implements an approved plan
model: opus
---
```

工作很小就用 `sonnet`。只有交出去的工作會在那個模型上跑。主工作階段自己寫的程式碼，仍然在 Fable 上跑。

## 做法三：把計劃貼到較便宜的工作階段

用 Fable 做計劃，把計劃複製出來，再用 Opus 或 Sonnet 開一個新的工作階段：

```bash
claude --model opus
```

工作較小的時候，同樣的一步是 `claude --model sonnet`。把計劃貼進去。另一個代理 CLI 也可以用同樣的方法貼。新的工作階段只知道你貼上的內容。

## 做法四：在 Coppice 裏按代理設定模型

[Coppice](/) 是一套桌面軟件，把程式設計代理放在同一塊工作看板上。就算用同一個 CLI，每個代理也可以有自己的模型。以 Settings 裏的深色模式開關這類工單為例，PM 可以用 Fable 5.2 做計劃，Frontend Engineer 可以用 Opus 5.5 或 Sonnet 來做。

1. [安裝 Coppice](/docs/install)。在終端裏像平常一樣登入 Claude Code，然後加入你的專案。
2. 建立一個 PM 代理，選 Claude Code，把模型設定為 Fable 5.2。
3. 在 Claude Code 上建立 Technical Lead（技術負責人）和 Frontend Engineer。把 Frontend Engineer 的模型設定為 Opus 5.5；改動很小就設定為 Sonnet。
4. 寫好工單，讓代理開始工作。PM 會用 Fable 寫計劃。你讀過之後，可以批准計劃，也可以要求修改。這次規劃不會改你的程式碼。
5. 程式碼準備好了，就審查 diff。

你批准之後，Technical Lead 會檢查計劃，並指派 Frontend Engineer。Frontend Engineer 那次執行會用你設定的模型，而且會自行開始。[核心概念](/docs/concepts)說明了每一步。

![新建代理的畫面，名稱是 PM。模型列表已開啟，選取的是 Claude Fable 5.2。](/blog/fable-for-planning/pm-fable.png "PM 的模型列表，選取的是 Claude Fable 5.2。")

![Frontend Engineer 的新建代理畫面。已選 Claude Code，並顯示 Ready，模型是 Claude Opus 5.5。](/blog/fable-for-planning/frontend-opus.png "Frontend Engineer 用 Claude Code，模型是 Claude Opus 5.5。")

## 選哪一種

- **只有一個 Claude Code 工作階段，而且計劃要留在 Fable：** 先 `/model fable`，再 `/model opus` 或 `/model sonnet`。Opus 和 Sonnet 已經足夠時，用 `opusplan`。
- **某一步交給較便宜的模型：** 做一個 subagent，把 `model` 設定為 `opus` 或 `sonnet`。
- **想把計劃放進新的工作階段：** 貼到 Opus 或 Sonnet 的工作階段。
- **工單多，做計劃的用 Fable，寫程式的用 Opus 或 Sonnet：** 用 Coppice。由[快速上手](/docs/getting-started)開始。
