---
title: 提供商
description: "Coppice 執行的代理 CLI：Claude Code、Codex、Cursor、OpenCode、Kilo Code、GitHub Copilot、Qwen Code、Kimi Code 和 Grok。"
---

# 提供商

受夠了為每個代理都開一個終端？Coppice 執行你已經在用的程式設計代理 CLI，並把它們的工作放到同一塊看板上。訂閲和登入都還是你自己的，不需要 Coppice 帳號。

<a id="supported-connectors"></a>

## 支援的聯結器

| 聯結器 | Coppice 查詢的命令 | 安裝 |
| --- | --- | --- |
| Claude Code | `claude` | [安裝指南](https://docs.anthropic.com/en/docs/claude-code/setup) |
| Codex | `codex` | [安裝指南](https://developers.openai.com/codex/cli) |
| Cursor | `cursor-agent` | [安裝指南](https://cursor.com/docs/cli/installation) |
| OpenCode | `opencode` | [安裝指南](https://opencode.ai/docs/) |
| Kilo Code | `kilo` | [安裝指南](https://kilo.ai/docs/cli) |
| GitHub Copilot | `copilot` | [安裝指南](https://docs.github.com/en/copilot/how-tos/copilot-cli) |
| Qwen Code | `qwen` | [安裝指南](https://github.com/QwenLM/qwen-code) |
| Kimi Code | `kimi` | [安裝指南](https://moonshotai.github.io/kimi-code/) |
| Grok | `grok` | [安裝指南](https://x.ai/cli) |

<figure>
<img src="/assets/docs/model-picker.webp" width="1440" height="900" alt="代理上的模型選擇器，列出該代理 CLI 提供的模型。">
<figcaption>選擇一個模型，或保留預設值。</figcaption>
</figure>

為這個代理選擇一個模型，或保留預設值。裝了新模型？點選 Refresh models（重新整理模型）。

<a id="connect-your-agent-cli"></a>

## 連線你的代理 CLI

1. 像平常一樣安裝 CLI，並在終端裏登入。
2. 建立代理時選擇你的代理 CLI，Coppice 就會啓用它。

代理表單會列出每個聯結器及其狀態：

| 狀態 | 含義 |
| --- | --- |
| **Ready** | 一次免費的登入檢查確認你已登入。 |
| **Found, not signed in** | Coppice 找到了命令，但你還沒有登入。請在終端裏登入。 |
| **Not on your PATH** | Coppice 找不到該命令。請安裝它，或檢查你的 PATH。 |
| **Installed** | Coppice 找到了命令。對於 Kilo Code、GitHub Copilot、Qwen Code 和 Kimi Code，不消耗你的套餐額度就無法檢查登入狀態。 |

Kilo Code、GitHub Copilot、Qwen Code 和 Kimi Code 顯示為 **Installed**。System → Connectors 中還會提示："Coppice can't check this sign-in without using your plan. A run will show an error if you're signed out."（不消耗你的套餐額度，Coppice 就無法檢查登入狀態；如果你已退出登入，執行時會報錯。）想確認它能用，就按 **Test connection**（測試連線）：它會跑一次簡短的聊天、規劃和工單。所有 CLI 都能聊天。Claude Code、Codex、Cursor 和 Kilo Code 的聊天會顯示 "Coppice hasn't confirmed that {cli} stays read-only. The agent can still run commands on your computer."（Coppice 尚未確認 {cli} 會保持只讀，代理仍可在你的電腦上執行命令。）OpenCode、GitHub Copilot、Qwen Code、Kimi Code 和 Grok 的聊天沒有這條橫幅：在聊天和規劃中，Coppice 會阻止它們寫入檔案和執行 shell 命令。你在這些 CLI 裏自己設定的 MCP 工具仍可能改動內容。

CLI 還沒就緒時，也可以先儲存代理。CLI 安裝並登入後，它就會開始處理工單。如果你是在開啓 Coppice 之後才安裝的 CLI，請退出並重新開啓 Coppice，讓它找到新命令。

Coppice 找到命令的聯結器會開啓，除非你未登入。要開啓或關閉某個聯結器，請使用 **System → Connectors**（系統 → 聯結器）中的開關。

<a id="use-your-own-api-key-or-another-provider"></a>

## 使用自己的 API 金鑰或其他提供商

想用自己的 API 金鑰，或者想用你的 CLI 不提供的模型？請用 OpenCode。它支援 75 個以上的模型提供商，也支援本地模型。參見 [OpenCode 的提供商列表](https://opencode.ai/docs/providers/)。

1. 安裝 OpenCode。
2. 在終端執行 `opencode`，輸入 `/connect`，選擇你的提供商並貼上金鑰。金鑰由 OpenCode 保管，Coppice 從不索要。列表裏沒有？請按 OpenCode 的[自定義提供商步驟](https://opencode.ai/docs/providers/#custom-provider)操作。
3. 在 Coppice 中建立一個代理，把 **Connector**（聯結器）設為 **OpenCode**。你的提供商的模型會出現在 **Model**（模型）下。如果你是在 Coppice 開啓期間新增的提供商，請點選 **Refresh models**（重新整理模型）。

<a id="on-a-ticket"></a>

## 在工單上

代理接手工單時，Coppice 會在該工單自己的 git worktree 中啓動它的 CLI。你可以在工單上即時檢視進度。

<a id="what-coppice-does-not-do"></a>

## Coppice 不做什麼
- 不提供雲端編碼服務
- 不會在未經你 Human Review（人工審查）的情況下合併程式碼
