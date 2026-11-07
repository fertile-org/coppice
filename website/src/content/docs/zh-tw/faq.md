---
title: 常見問題
metaTitle: "Coppice 常見問題：Beta、Docker、Windows 與合併"
description: "Coppice 能用於正式環境嗎？需要 Docker 或帳號嗎？代理會不經你同意就合併嗎？關於 Beta、Windows、衝突等問題的解答。"
---

# 常見問題

<a id="is-coppice-production-ready"></a>

## Coppice 可以用於正式環境了嗎？

還不行。Coppice 正處於 Beta，給想要一塊本機代理看板、而且真正由人把關的早期使用者。代理以你的本機權限執行。

<a id="do-i-need-docker-or-an-account"></a>

## 需要 Docker 或帳號嗎？

不需要。下載桌面應用，開啟就能用。不需要 Docker，不需要設定伺服器，也不需要 Coppice 帳號。

<a id="is-this-a-jira--linear-replacement"></a>

## 它能替代 Jira / Linear 嗎？

不能。Coppice 是一塊用來驅動程式設計代理的看板，不是給整個公司用的完整問題追蹤系統。

<a id="is-this-a-cloud-coding-agent"></a>

## 這是雲端程式設計代理嗎？

不是。代理透過你電腦上的 CLI，在你的倉庫裡執行。差別在於：雲端代理服務把工作交到別人的電腦；Coppice 讓工作留在本機，並由你把關。

<a id="will-agents-merge-without-me"></a>

## 代理會不經過我就合併嗎？

不會。Human Review（人工審查）是 Coppice 的核心關卡。你開啟 diff，由你決定。

<a id="what-happens-if-a-tickets-changes-conflict-with-main"></a>

## 如果工單的改動和 main 衝突怎麼辦？

Coppice 會在合併前停下，不動 main。它會列出衝突的檔案，並把工單送回 In Progress，由同一個代理開始解決衝突。之後你需要在 Human Review 中再接受一次，才會合併。

<a id="what-about-set-and-forget-autopilots"></a>

## 「設好就不用管」的自動駕駛呢？

那不是 Coppice 要做的產品。Coppice 是用來管理代理的：工作由你發起，每個改動都由你接受。

<a id="windows"></a>

## Windows 呢？

Windows 11 應該可以透過 WSL 執行 Linux x64 版本，但我們還沒有測試過。參見 [Windows 11（透過 WSL）](/docs/install#windows-11-through-wsl)。如果你試了，歡迎[在 GitHub 上告訴我們結果](https://github.com/fertile-org/coppice/issues)。這次 Beta 沒有計劃推出原生 Windows 應用。

<a id="where-do-contributors-look"></a>

## 貢獻者該看哪裡？

架構、開發和測試文件都在 [GitHub 倉庫](https://github.com/fertile-org/coppice/tree/main/docs)。這個網站說明如何使用 Coppice。
