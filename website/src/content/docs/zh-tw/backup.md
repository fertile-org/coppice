---
title: 備份資料
metaTitle: "備份和遷移 Coppice 資料"
description: "下載 Coppice 備份並在新電腦上匯入，或手動複製資料夾，把資料遷到另一台電腦。如果備份外流，可以更換金鑰。"
---

# 備份資料

<a id="download-a-backup"></a>

## 下載備份

開啟 **System**（系統），再開啟 **Backup**（備份），選擇 **Download backup**（下載備份）。你會得到一個檔案，裡面有看板、工單、設定、檔案和代理 worktree。

密碼、金鑰和已儲存的權杖不會包含在內。備份仍然包含你全部的工作區資料，請妥善保管，不要外傳。

<a id="import-a-backup"></a>

## 匯入備份

匯入會用備份裡的資料，替換那台電腦上的全部資料。看板和工作區設定會一起搬過來。密碼、金鑰、資料夾路徑、代理命令，以及外掛和 MCP 設定不會遷移，需要重新設定。你可能會被登出。

在要遷入的電腦上，開啟 **System**，再開啟 **Backup**，選擇 **Import backup**（匯入備份）。

已儲存的權杖不在備份中，所以之後要逐個重新貼上。Coppice 會在每個需要的權杖旁標註 "Paste this token again."（請重新貼上此權杖。）貼上後儲存即可。

<a id="copy-the-folders-by-hand"></a>

## 手動複製資料夾

你也可以先結束 Coppice，再複製它的資料資料夾：

| | 資料 | 日誌 |
| --- | --- | --- |
| macOS | `~/Library/Application Support/Coppice` | `~/Library/Logs/Coppice` |
| Linux | `~/.config/Coppice` | `~/.local/state/Coppice/logs` |

這份副本包含 `secrets` 資料夾，請把它當成密碼一樣保管。CLI 的登入資訊不在裡面，每個 CLI 各自儲存。

<a id="replace-keys"></a>

## 更換金鑰

如果備份分享出去了，或你懷疑別人有一份副本，請產生新的金鑰。

開啟 **System**，再開啟 **Backup**，選擇 **Replace keys**（更換金鑰）。輸入新的管理員密碼，並勾選確認框。Coppice 會產生新的金鑰和資料庫密碼。已儲存的權杖仍然可用，所有人都會被登出。
