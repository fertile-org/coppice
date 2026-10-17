---
title: 备份数据
description: "备份 Coppice、把它迁移到新电脑，以及在备份泄露时更换密钥。"
---

# 备份数据

<a id="download-a-backup"></a>

## 下载备份

打开 **Tools**（工具），再打开 **Backup**（备份），选择 **Download backup**（下载备份）。你会得到一个文件，里面包含你的看板、工单、设置、文件和智能体 worktree。

密码、密钥和已保存的令牌不会包含在内。但备份仍然包含你全部的工作区数据，请妥善保管，不要外传。

<a id="import-a-backup"></a>

## 导入备份

在要迁移到的电脑上，打开 **Tools**，再打开 **Backup**，选择 **Import backup**（导入备份）。

导入会用备份中的数据替换那台电脑上的全部数据。你的看板和工作区设置会迁移过来。密码、密钥、文件夹路径、智能体命令，以及插件和 MCP 设置不会迁移，需要重新设置。你可能会被退出登录。

已保存的令牌不在备份中，所以之后需要逐个重新粘贴。Coppice 会在每个需要的令牌旁标注 "Paste this token again."（请重新粘贴此令牌。）粘贴后保存即可。

<a id="copy-the-folders-by-hand"></a>

## 手动复制文件夹

你也可以退出 Coppice，然后复制它的数据文件夹：

| | 数据 | 日志 |
| --- | --- | --- |
| macOS | `~/Library/Application Support/Coppice` | `~/Library/Logs/Coppice` |
| Linux | `~/.config/Coppice` | `~/.local/state/Coppice/logs` |

这份副本包含 `secrets` 文件夹，请像对待密码一样对待它。你的 CLI 登录信息不在其中，每个 CLI 各自保存。

<a id="replace-keys"></a>

## 更换密钥

如果你分享过备份，或者怀疑别人有一份副本，请生成新的密钥。

打开 **Tools**，再打开 **Backup**，选择 **Replace keys**（更换密钥）。输入新的管理员密码并勾选确认框。Coppice 会生成新的密钥和数据库密码。已保存的令牌仍然可用，所有人都会被退出登录。
