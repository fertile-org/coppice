---
title: 備份資料
description: "備份 Coppice、把它遷移到新電腦，以及在備份泄露時更換金鑰。"
---

# 備份資料

<a id="download-a-backup"></a>

## 下載備份

開啓 **System**（系統），再開啓 **Backup**（備份），選擇 **Download backup**（下載備份）。你會得到一個檔案，裏面包含你的看板、工單、設定、檔案和代理 worktree。

密碼、金鑰和已儲存的令牌不會包含在內。但備份仍然包含你全部的工作區資料，請妥善保管，不要外傳。

<a id="import-a-backup"></a>

## 匯入備份

在要遷移到的電腦上，開啓 **System**，再開啓 **Backup**，選擇 **Import backup**（匯入備份）。

匯入會用備份中的資料替換那台電腦上的全部資料。你的看板和工作區設定會遷移過來。密碼、金鑰、資料夾路徑、代理命令，以及外掛和 MCP 設定不會遷移，需要重新設定。你可能會被退出登入。

已儲存的令牌不在備份中，所以之後需要逐個重新貼上。Coppice 會在每個需要的令牌旁標註 "Paste this token again."（請重新貼上此令牌。）粘貼後儲存即可。

<a id="copy-the-folders-by-hand"></a>

## 手動複製資料夾

你也可以退出 Coppice，然後複製它的資料資料夾：

| | 資料 | 日誌 |
| --- | --- | --- |
| macOS | `~/Library/Application Support/Coppice` | `~/Library/Logs/Coppice` |
| Linux | `~/.config/Coppice` | `~/.local/state/Coppice/logs` |

這份副本包含 `secrets` 資料夾，請像對待密碼一樣對待它。你的 CLI 登入資訊不在其中，每個 CLI 各自儲存。

<a id="replace-keys"></a>

## 更換金鑰

如果你分享過備份，或者懷疑別人有一份副本，請生成新的金鑰。

開啓 **System**，再開啓 **Backup**，選擇 **Replace keys**（更換金鑰）。輸入新的管理員密碼並勾選確認框。Coppice 會生成新的金鑰和數據庫密碼。已儲存的令牌仍然可用，所有人都會被退出登入。
