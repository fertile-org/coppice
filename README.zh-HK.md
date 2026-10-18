[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文（台灣）](README.zh-TW.md) · [繁體中文（香港）](README.zh-HK.md) · [日本語](README.ja.md) · [Français](README.fr.md)

# Coppice

**Beta 版。** 僱用專家，你來拍板。

<p align="center">
  <img src="static/marketing.gif" alt="寫一個工單，批准計劃，審查 diff。你接受之前，什麼都不算完成。" width="960" />
</p>

**不用再在各個代理終端之間來回切換。** 寫一個工單，你的 PM 代理會起草一份計劃等你批准。然後 Claude Code、Codex 等代理並行構建、審查和測試，各自在自己的分支上工作。看板上能看到哪些在推進、哪些卡住了、哪些在等你；在你審查 diff 之前，什麼都不算完成。它在你的電腦上執行，用的是你已經付費的代理 CLI：不用新訂閲，也不用 Coppice 帳號。

## 兩道關卡

工作會在兩個地方停下來等你：

1. **Plan Review**（計劃審查）。你的 PM 代理起草計劃，在開工之前由你批准或要求修改。規劃永遠不會改動工單的程式碼。開啓 Skip planning（跳過規劃）後，工單會直接進入 Ready。
2. **Wait for Human Review**（等待人工審查）。由你審查 diff。在你接受之前，工單會一直停在這一列。

工作由你發起。建立一個工單，或批准一個代理建議的工單。在這兩道關卡之間，由代理負責規劃和構建。

## 本次 Beta

Beta 版的安裝包包括 macOS 的 `.dmg`（Apple 晶片或 Intel）和 Linux 的 `.deb`（`amd64` 或 `arm64`）。請從[釋出頁面](https://github.com/fertile-org/coppice/releases)下載。

帶上你已經在用的 CLI：Claude Code、Codex、Cursor、OpenCode 和 Kilo Code。按常規方式安裝並登入即可。Coppice 會在你登入 shell 的 `PATH` 中找到它們。

## 開始使用

產品文件就是 [`website/`](website/) 中的網站：落地頁和使用者指南。公開網站地址是 [https://getcoppice.app/](https://getcoppice.app/)。

## 貢獻者

貢獻者和 CI 都通過 Docker Compose 執行整套服務。請先閲讀 [AGENTS.md](AGENTS.md) 和 [docs/development.md](docs/development.md)。路線圖見 [docs/milestones/README.md](docs/milestones/README.md)。

## 許可證

Coppice 基於 [Apache 2.0 許可證](LICENSE)開源。
