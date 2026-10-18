---
title: 安裝
description: "Coppice Beta 以桌面應用的形式釋出。不需要 Docker，不需要伺服器，也不需要 Coppice 帳號。"
---

# 安裝

Coppice Beta 以桌面應用的形式釋出。不需要 Docker，不需要伺服器，也不需要 Coppice 帳號。

<a id="supported-platforms"></a>

## 支援的平臺
| 平臺 | 安裝包 |
| --- | --- |
| macOS (Apple Silicon) | `.dmg` |
| Linux (x64) | `.deb` |

本次 Beta 沒有 Windows 應用。如果你用的是 Windows 11，請看 [Windows 11（通過 WSL）](#windows-11-through-wsl)。

<a id="download"></a>

## 下載
- [macOS (Apple Silicon) `.dmg`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-arm64.dmg)
- [macOS (Intel) `.dmg`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-x64.dmg)
- [Linux (x64) `.deb`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-amd64.deb)
- [Linux (ARM64) `.deb`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-arm64.deb)

所有版本也都可以在 [GitHub Releases](https://github.com/fertile-org/coppice/releases) 獲取。

<a id="updates"></a>

## 更新

Coppice 會在後臺檢查新版本。如果想關閉，開啟 **Settings → Updates**（設定 → 更新），關掉 **Automatic updates**（自動更新）。更新後，你的看板和設定都會保留。

- **macOS：** 在橫幅中選擇 **Download update**（下載更新），然後把新的 Coppice 拖進“應用程式”資料夾。
- **Linux：** 安裝 `.deb` 時會新增 Coppice 的軟體源，所以新版本會和其他更新一起出現在“軟體更新器”裡；也可以執行 `sudo apt update && sudo apt upgrade`。

<a id="windows-11-through-wsl"></a>

## Windows 11（通過 WSL）
Linux x64 版本應該可以通過 WSL 在 Windows 11 上執行，但我們還沒有測試過。如果你試了，歡迎[告訴我們結果](https://github.com/fertile-org/coppice/issues)。

你需要 Windows 11，因為較早的 Windows 10 版本無法顯示 Linux 應用視窗。目前暫不支援 Arm 架構的 Windows 電腦。

1. 以管理員身份開啟 PowerShell，執行 `wsl --install`。這會安裝 Ubuntu。按提示重啟，然後完成 Ubuntu 使用者的設定。
2. 仍以管理員身份在 PowerShell 中更新 WSL，讓 Linux 應用視窗能顯示在 Windows 桌面上：

   ```powershell
   wsl --update
   wsl --shutdown
   ```

3. 從“開始”選單開啟 Ubuntu。後面的步驟都在 Ubuntu 中完成。下載並安裝 Linux x64 的 `.deb`。把 `<version>` 換成 [GitHub Releases](https://github.com/fertile-org/coppice/releases) 上的最新版本號：

   ```sh
   cd ~
   wget https://github.com/fertile-org/coppice/releases/download/v<version>/Coppice-<version>-linux-amd64.deb
   sudo apt install ./Coppice-<version>-linux-amd64.deb
   ```

   請用 `apt`，不要用 `dpkg -i`。`apt` 會順帶安裝 Coppice 需要的依賴。

   以後要更新 Coppice，執行 `sudo apt update && sudo apt upgrade`。你的看板和設定都會保留。
4. 在 Ubuntu 中安裝你要用的代理 CLI（Claude Code、Codex、Cursor、OpenCode、Kilo Code），並在 Ubuntu 裡登入這些 CLI 和 git。
5. 從 Windows“開始”選單（Ubuntu 分組下）啟動 Coppice，或在 Ubuntu 中執行 `coppice`。

   如果某個 CLI 顯示 Not on your PATH，請在 Ubuntu 中執行 `coppice` 來啟動 Coppice。

WSL 會把 Windows 的 PATH 加到 Ubuntu 中，所以 Coppice 可能會找到某個 CLI 的 Windows 版本。開啟 **System → Connectors**（系統 → 聯結器），檢視每個聯結器的 CLI 路徑。如果路徑以 `/mnt/c/` 開頭，請在 Ubuntu 中安裝該 CLI。

把倉庫放在 Linux 主目錄下，例如 `~/code/my-app`，不要放在 `/mnt/c/` 下。跨這道邊界訪問時，Git 和檔案讀寫都會慢很多。資料夾選擇器顯示的是 Linux 路徑。

<a id="first-run"></a>

## 首次執行
1. 安裝並開啟 Coppice。
2. 選擇你磁碟上已有的一個本地 git 倉庫。
3. **連線你的代理 CLI。** 建立代理時選擇你的代理 CLI，Coppice 就會啟用它。每個 CLI 都會顯示狀態：**Ready**（就緒）、**Found, not signed in**（已找到，未登入）或 **Not on your PATH**（不在 PATH 中）。請先像平常一樣在終端裡登入該 CLI。

   Kilo Code 顯示為 **Kilo Code — Installed**。System → Connectors 中還會提示："Coppice can't check this sign-in without using your plan. A run will show an error if you're signed out."（不消耗你的套餐額度，Coppice 就無法檢查登入狀態；如果你已退出登入，執行時會報錯。）

   以後要關閉某個聯結器，可以在 System → Connectors 中使用開關，或在 Coppice 的 `config.toml` 中為它設定 `enabled = false`：

   - macOS：`~/Library/Application Support/Coppice/config.toml`
   - Linux：`~/.config/Coppice/config.toml`

4. 建立一個工單（或批准一個建議的工單），把它移到 Ready。代理會帶著它依次經過 In Progress、In Review 和 In QA，然後等待你進行 Human Review（人工審查）。

<a id="requirements"></a>

## 系統要求
- 一個你有寫許可權的本地 git 倉庫（Coppice 使用隔離的 worktree）
- 至少安裝一個受支援的代理 CLI，並按該工具的常規方式完成登入認證

<a id="troubleshooting"></a>

## 故障排查
如果應用無法啟動，或者代理一直不接工單，請檢視工單上的即時終端和常見問題。如需從原始碼構建，請看倉庫中的[貢獻者文件](https://github.com/fertile-org/coppice/blob/main/docs/development.md)。
