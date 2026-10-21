---
title: 設定參考
description: "config.toml 中所有可修改的欄位，以及它們的型別和預設值。"
---

# 設定參考

Coppice 把設定儲存在一個檔案 `config.toml` 中：

| 作業系統 | 路徑 |
| --- | --- |
| Linux | `~/.config/Coppice/config.toml` |
| macOS | `~/Library/Application Support/Coppice/config.toml` |

在 Linux 上，如果設定了 `XDG_CONFIG_HOME`，檔案位於 `$XDG_CONFIG_HOME/Coppice/config.toml`。

<a id="edit-it-in-the-app"></a>

## 在應用中編輯
在側邊欄中開啓 **Settings**（設定）。頁面會以文本形式顯示該檔案，你寫的註釋也會保留。

- **Save** 會在寫入前檢查檔案。無效的值不會被儲存。
- **Restore last good version** 可以撤銷一次錯誤的修改。

有些修改會立即生效，另一些會提示 "Saved. Restart Coppice to apply."（已儲存，重啓 Coppice 後生效。）

你也可以用任意文本編輯器修改這個檔案。你的修改在升級後依然保留。

<a id="updates"></a>

## 更新
開啓 **Settings → Updates**（設定 → 更新）可以檢視當前版本，並選擇 **Check for updates**（檢查更新）。**Automatic updates**（自動更新）預設開啓。關閉後，Coppice 將不再在後台檢查新版本。

這個開關不在 `config.toml` 中。Coppice 把它以 `automaticUpdates` 儲存在同一資料夾下的 `update-settings.json` 中。

<a id="replace-keys"></a>

## 更換金鑰

如果你分享過備份，請生成新的金鑰和新的管理員密碼。已儲存的令牌仍然可用，所有人都會被退出登入。

開啓 **System → Backup → Replace keys**，輸入新的管理員密碼並確認。

<a id="logs"></a>

## 日誌
管理員可以開啓 **System → Logs**（系統 → 日誌），篩選並即時檢視伺服器、請求、代理執行和數據庫日誌。每次執行都有一個 **View logs** 連結。

**Export logs** 會把符合當前篩選條件的日誌行儲存為 `coppice-logs.zip`。Coppice 會移除它已知的令牌和金鑰，但分享之前請自己再檢查一遍。

**Report a problem** 會新建一個 GitHub issue，並自動填好你的 Coppice 版本。寫明發生了什麼、你原本期望什麼，然後附上通過 **Export logs** 匯出的 zip 檔案。

<h2 id="logs-1"><code>[logs]</code></h2>

| 欄位 | 型別 | 預設值 | 作用 |
| --- | --- | --- | --- |
| `retention_days` | 整數 | `14` | 日誌保留天數（含當天）：3、7、14 或 30，預設 14。Coppice 每小時刪除一次過期日誌，修改無需重啓即可生效。如果日誌超過 256 MiB，即使還沒到 `retention_days`，Coppice 也會從最舊的開始刪除。 |

即使超過限制，Coppice 也不會刪除當天的日誌，以及正在寫入的數據庫日誌。數據庫日誌計入 256 MiB 上限，超過 `retention_days` 後會被刪除。

| 欄位 | 型別 | 預設值 | 作用 |
| --- | --- | --- | --- |
| `dir` | 字串 | `./data/logs` | 日誌資料夾。桌面應用使用自己的資料夾，會忽略此項。修改後需要重啓。 |

<a id="connectors"></a>

## 聯結器
Coppice 找到的 CLI 預設開啓，除非登入檢查顯示你已退出登入。只有想關閉某個聯結器時，才需要加一行：

```toml
[agent.connectors.claude-code]
enabled = false
```

**System → Connectors** 中的開關寫入的也是這一行。

Kilo Code、GitHub Copilot、Qwen Code 和 Kimi Code 只要被 Coppice 找到就會開啓。Coppice 無法免費檢查它們的登入狀態，所以如果你已退出登入，執行時會報錯。

## `[agent]`

| 欄位 | 型別 | 預設值 | 作用 |
| --- | --- | --- | --- |
| `worktrees_path` | 字串 | `<data-dir>/worktrees` | 存放 worktree 的資料夾，每個工單各有一個 git worktree。 |
| `worker_count` | 整數 | `2` | 同時執行的代理任務數。小於 1 時按 1 處理。 |
| `health_check_interval_secs` | 整數 | `60` | 代理健康檢查的間隔秒數。小於 10 時按 10 處理。 |

`<data-dir>` 是存放 `config.toml` 的資料夾：Linux 和 WSL 上為 `~/.config/Coppice`，macOS 上為 `~/Library/Application Support/Coppice`。

## `[agent.connectors.<id>]`

可用的 id：`claude-code`、`codex`、`cursor`、`opencode`、`kilo-code`、`copilot`、`qwen-code`、`kimi`、`grok`。每個表都是可選的。

| 欄位 | 型別 | 預設值 | 作用 |
| --- | --- | --- | --- |
| `enabled` | 布林值 | 未設定：CLI 已登入則開啓 | 開啓或關閉該聯結器。 |
| `model_providers` | 字串列表 | `[]` | 該聯結器的模型提供商。開啓聯結器時自動填寫。 |
| `run_timeout_secs` | 整數 | OpenCode 為 `1800`，其他為 `600` | 該聯結器上的一次執行超過多少秒算失敗。 |
| `command` | 字串 | `cursor-agent`（Cursor）、`codex`、`opencode`、`kilo`、`copilot`、`qwen`、`kimi`、`grok` | 要執行的 CLI 執行檔。Claude Code 始終執行 `claude`，沒有 `command`。 |

僅限 OpenCode：

| 欄位 | 型別 | 預設值 | 作用 |
| --- | --- | --- | --- |
| `serve_hostname` | 字串 | `127.0.0.1` | 執行期間 OpenCode 輔助程序使用的主機名。修改後需要重啓。 |

修改 OpenCode 的 `command` 也需要重啓。

## `[workflow]`

| 欄位 | 型別 | 預設值 | 作用 |
| --- | --- | --- | --- |
| `auto_start_runs` | 布林值 | `true` | 為 true 時，給已關聯倉庫的工單指派代理會自動開始執行。為 false 時，需要你點選 **Run agent**。 |
| `auto_assign.default` | 布林值 | `true` | 為所有沒有單獨設定的狀態開啓自動指派。 |
| `auto_assign.<status>` | 布林值 | `backlog`：`false`；其他：使用 `default` | 為某個狀態開啓或關閉自動指派。 |
| `auto_split.default` | 布林值 | `false` | 為所有沒有單獨設定的狀態開啓自動拆分。 |
| `auto_split.<status>` | 布林值 | 使用 `default` | 為某個狀態開啓或關閉自動拆分。 |

狀態鍵：`backlog`、`ready`、`in_progress`、`in_review`、`in_qa`、`wait_for_final_review`（Wait for Human Review）、`blocked`、`done`。

某個狀態開啓自動指派時，Coppice 會指派該次執行指定的代理。關閉時，工單上會改為顯示 **Recommends:**（推薦）。

某個狀態開啓自動拆分時，Coppice 會建立該次執行建議的子工單。關閉時，它會儲存一條 **Pending split recommendation**（待處理的拆分建議），直到你點選 **Approve splits**。

## `[knowledge]`

| 欄位 | 型別 | 預設值 | 作用 |
| --- | --- | --- | --- |
| `enabled` | 布林值 | `true` | 開啓或關閉知識壓縮整理。 |
| `poll_interval_ms` | 整數 | `500` | 壓縮排程器的喚醒頻率。小於 100 時按 100 處理。 |
| `compaction.interval_secs` | 整數 | `1800` | 兩輪壓縮之間的間隔秒數。 |
| `compaction.batch_max_tickets` | 整數 | `10` | 一批最多包含的工單數。 |
| `compaction.batch_max_source_bytes` | 整數 | `200000` | 一批最多包含的源資料位元組數。 |
| `compaction.max_candidates_per_batch` | 整數 | `20` | 一批最多能提出的候選條目數。 |
| `compaction.max_attempts` | 整數 | `3` | 一批最多嘗試的次數。 |
| `auto_save.enabled` | 布林值 | `false` | 不經詢問，直接儲存低風險的候選條目。 |
| `auto_save.allowed_types` | 字串列表 | `[]` | 自動儲存允許儲存的型別：`bug_pattern`、`coding_convention`、`dependency_note`、`performance_note`、`review_feedback`、`test_command`。 |
| `auto_save.minimum_confidence` | 字串 | `high` | 必須保持為 `high`。 |
| `retrieval.top_k` | 整數 | `8` | 一次查詢返回的條目數，1 到 20。 |
| `retrieval.allowed_types` | 字串列表 | `[]`（所有型別） | 只返回這些型別：`coding_convention`、`architecture_rule`、`bug_pattern`、`test_command`、`review_feedback`、`dependency_note`、`api_contract`、`workflow_rule`、`human_preference`、`operational_runbook`、`security_rule`、`performance_note`。 |
| `retrieval.minimum_confidence` | 字串 | `medium` | `low`、`medium` 或 `high`。 |
| `retrieval.default_page_size` | 整數 | `25` | 預設每頁條數。 |
| `retrieval.max_page_size` | 整數 | `100` | 每頁最大條數，最多 100。 |
| `retrieval.max_active_per_board` | 整數 | `1000` | 一塊看板最多可容納的 Knowledge 條目數（待批准或已批准）。該看板上代理的條目也計入在內。 |
| `retrieval.max_active_workspace` | 整數 | `10000` | 適用於整個工作區的 Knowledge 條目上限（待批准或已批准）。 |

以上所有數量和間隔都必須大於 0。

## `[git]`

| 欄位 | 型別 | 預設值 | 作用 |
| --- | --- | --- | --- |
| `author_name` | 字串 | `Coppice` | Coppice 所做提交的作者名。 |
| `author_email` | 字串 | `coppice@localhost` | 這些提交的作者郵箱。 |
| `push_enabled` | 布林值 | `false`（桌面應用程式為 `true`） | 允許 Coppice 推送分支並建立 pull request。 |

## `[plugins]`

| 欄位 | 型別 | 預設值 | 作用 |
| --- | --- | --- | --- |
| `dir` | 字串 | `<data-dir>/plugins` | 你的預設外掛資料夾。更多資料夾可在 Plugins 頁面新增。 |
| `allow_file_git_urls` | 布林值 | `false` | 允許安裝外掛時使用 `file:` 形式的 git URL。 |
| `git_timeout_secs` | 整數 | `300` | 外掛克隆或更新超過多少秒算失敗。 |
| `mcp_start_timeout_secs` | 整數 | `20` | 等待外掛 MCP 伺服器啓動的秒數。 |
| `mcp_list_timeout_secs` | 整數 | `10` | 等待外掛列出工具的秒數。 |
| `mcp_idle_shutdown_secs` | 整數 | `600` | 空閒的外掛 MCP 伺服器保持執行的秒數。 |

## `[mcp]`

| 欄位 | 型別 | 預設值 | 作用 |
| --- | --- | --- | --- |
| `builtin_plugins_dir` | 字串 | `<data-dir>/builtin-plugins` | Coppice 每次啓動時寫入內建技能的位置。在這裏做的修改會丟失。 |
| `call_timeout_secs` | 整數 | `60` | 單次工具呼叫超過多少秒算失敗。 |
| `max_output_bytes` | 整數 | `32768` | 工具結果的最大位元組數。 |
| `comment_post_limit` | 整數 | `5` | 一次執行最多能發表的評論數。 |
| `token_ttl_secs` | 整數 | `14400` | 一次執行的工具令牌有效期（秒）。 |

## `[keybindings]`

每個鍵都是[鍵盤快捷鍵](/docs/shortcuts)中的一個命令 id。你設定的按鍵會替換該命令的預設按鍵，設為空值則關閉該快捷鍵。

| 欄位 | 型別 | 預設值 | 作用 |
| --- | --- | --- | --- |
| `"<command id>"` | 字串或字串列表 | 內建快捷鍵 | 替換該命令的按鍵。`""` 會移除它們。 |

```toml
[keybindings]
"palette.open" = "P"
"ticket.create" = ""
"board.nextCard" = ["J", "Down"]
```

`Mod` 在 macOS 上是 Cmd，在 Linux 和 WSL 上是 Ctrl。按鍵序列寫作 `G then B`。一個命令的多個按鍵用 ` or ` 分隔，或寫成列表。儲存後無需重啓即可生效。無法解析的快捷鍵、未知的命令 id、已被其他命令佔用的按鍵，以及屬於瀏覽器或系統的按鍵都會被跳過。該命令保留預設按鍵，Settings 會説明原因。
