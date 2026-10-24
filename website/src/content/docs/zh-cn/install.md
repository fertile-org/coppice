---
title: 安装
metaTitle: "在 macOS、Linux 和 Windows（WSL）上安装 Coppice"
description: "在 macOS、Linux 或通过 WSL 在 Windows 11 上安装 Coppice 桌面应用。无需 Docker、服务器或账号。包括更新、系统要求和故障排查。"
---

# 安装

Coppice Beta 以桌面应用的形式发布。不需要 Docker，不需要服务器，也不需要 Coppice 账号。

<a id="supported-platforms"></a>

## 支持的平台
| 平台 | 安装包 |
| --- | --- |
| macOS (Apple Silicon) | `.dmg` |
| Linux (x64) | `.deb` |

本次 Beta 没有 Windows 应用。如果你用的是 Windows 11，请看 [Windows 11（通过 WSL）](#windows-11-through-wsl)。

<a id="download"></a>

## 下载
- [macOS (Apple Silicon) `.dmg`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-arm64.dmg)
- [macOS (Intel) `.dmg`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-x64.dmg)
- [Linux (x64) `.deb`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-amd64.deb)
- [Linux (ARM64) `.deb`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-arm64.deb)

所有版本也都可以在 [GitHub Releases](https://github.com/fertile-org/coppice/releases) 获取。

<a id="updates"></a>

## 更新

Coppice 会在后台检查新版本。如果想关闭，打开 **System → Updates**（系统 → 更新），关掉 **Automatic updates**（自动更新）。更新后，你的看板和设置都会保留。

- **macOS：** Coppice 会替你下载更新。在横幅中选择 **Restart to update**（重启以更新），或者在你下次退出 Coppice 时自动安装。
- **Linux：** 安装 `.deb` 时会添加 Coppice 的软件源，所以新版本会和其他更新一起出现在“软件更新器”里；也可以运行 `sudo apt update && sudo apt upgrade`。

<a id="windows-11-through-wsl"></a>

## Windows 11（通过 WSL）
Linux x64 版本应该可以通过 WSL 在 Windows 11 上运行，但我们还没有测试过。如果你试了，欢迎[告诉我们结果](https://github.com/fertile-org/coppice/issues)。

你需要 Windows 11，因为较早的 Windows 10 版本无法显示 Linux 应用窗口。目前暂不支持 Arm 架构的 Windows 电脑。

1. 以管理员身份打开 PowerShell，运行 `wsl --install`。这会安装 Ubuntu。按提示重启，然后完成 Ubuntu 用户的设置。
2. 仍以管理员身份在 PowerShell 中更新 WSL，让 Linux 应用窗口能显示在 Windows 桌面上：

   ```powershell
   wsl --update
   wsl --shutdown
   ```

3. 从“开始”菜单打开 Ubuntu。后面的步骤都在 Ubuntu 中完成。下载并安装 Linux x64 的 `.deb`。把 `<version>` 换成 [GitHub Releases](https://github.com/fertile-org/coppice/releases) 上的最新版本号：

   ```sh
   cd ~
   wget https://github.com/fertile-org/coppice/releases/download/v<version>/Coppice-<version>-linux-amd64.deb
   sudo apt install ./Coppice-<version>-linux-amd64.deb
   ```

   请用 `apt`，不要用 `dpkg -i`。`apt` 会顺带安装 Coppice 需要的依赖。

   以后要更新 Coppice，运行 `sudo apt update && sudo apt upgrade`。你的看板和设置都会保留。
4. 在 Ubuntu 中安装你要用的智能体 CLI（Claude Code、Codex、Cursor、OpenCode、Kilo Code、GitHub Copilot、Qwen Code、Kimi Code、Grok），并在 Ubuntu 里登录这些 CLI 和 git。
5. 从 Windows“开始”菜单（Ubuntu 分组下）启动 Coppice，或在 Ubuntu 中运行 `coppice`。

   如果某个 CLI 显示 Not on your PATH，请在 Ubuntu 中运行 `coppice` 来启动 Coppice。

WSL 会把 Windows 的 PATH 加到 Ubuntu 中，所以 Coppice 可能会找到某个 CLI 的 Windows 版本。打开 **System → Connectors**（系统 → 连接器），查看每个连接器的 CLI 路径。如果路径以 `/mnt/c/` 开头，请在 Ubuntu 中安装该 CLI。

把仓库放在 Linux 主目录下，例如 `~/code/my-app`，不要放在 `/mnt/c/` 下。跨这道边界访问时，Git 和文件读写都会慢很多。文件夹选择器显示的是 Linux 路径。

<a id="first-run"></a>

## 首次运行
1. 安装并打开 Coppice。
2. 选择你磁盘上已有的一个本地 git 仓库。
3. **连接你的智能体 CLI。** 创建智能体时选择你的智能体 CLI，Coppice 就会启用它。每个 CLI 都会显示状态：**Ready**（就绪）、**Found, not signed in**（已找到，未登录）或 **Not on your PATH**（不在 PATH 中）。请先像平常一样在终端里登录该 CLI。

   Kilo Code、GitHub Copilot、Qwen Code 和 Kimi Code 显示为 **Installed**（例如 **Kilo Code — Installed**）。System → Connectors 中还会提示："Coppice can't check this sign-in without using your plan. A run will show an error if you're signed out."（不消耗你的套餐额度，Coppice 就无法检查登录状态；如果你已退出登录，运行时会报错。）

   以后要关闭某个连接器，可以在 System → Connectors 中使用开关，或在 Coppice 的 `config.toml` 中为它设置 `enabled = false`：

   - macOS：`~/Library/Application Support/Coppice/config.toml`
   - Linux：`~/.config/Coppice/config.toml`

4. 创建一个工单（或批准一个建议的工单），把它移到 Ready。智能体会带着它依次经过 In Progress、In Review 和 In QA，然后等待你进行 Human Review（人工审核）。

<a id="requirements"></a>

## 系统要求
- 一个你有写权限的本地 git 仓库（Coppice 使用隔离的 worktree）
- 至少安装一个受支持的智能体 CLI，并按该工具的常规方式完成登录认证

<a id="troubleshooting"></a>

## 故障排查
如果应用无法启动，或者智能体一直不接工单，请查看工单上的实时终端和常见问题。如需从源码构建，请看仓库中的[贡献者文档](https://github.com/fertile-org/coppice/blob/main/docs/development.md)。
