---
title: 设置参考
description: "config.toml 中所有可修改的字段，以及它们的类型和默认值。"
---

# 设置参考

Coppice 把设置保存在一个文件 `config.toml` 中：

| 操作系统 | 路径 |
| --- | --- |
| Linux | `~/.config/Coppice/config.toml` |
| macOS | `~/Library/Application Support/Coppice/config.toml` |

在 Linux 上，如果设置了 `XDG_CONFIG_HOME`，文件位于 `$XDG_CONFIG_HOME/Coppice/config.toml`。

<a id="edit-it-in-the-app"></a>

## 在应用中编辑
在侧边栏中打开 **Settings**（设置）。页面会以文本形式显示该文件，你写的注释也会保留。

- **Save** 会在写入前检查文件。无效的值不会被保存。
- **Restore last good version** 可以撤销一次错误的修改。

有些修改会立即生效，另一些会提示 "Saved. Restart Coppice to apply."（已保存，重启 Coppice 后生效。）

你也可以用任意文本编辑器修改这个文件。你的修改在升级后依然保留。

<a id="updates"></a>

## 更新
打开 **Settings → Updates**（设置 → 更新）可以查看当前版本，并选择 **Check for updates**（检查更新）。**Automatic updates**（自动更新）默认开启。关闭后，Coppice 将不再在后台检查新版本。

这个开关不在 `config.toml` 中。Coppice 把它以 `automaticUpdates` 保存在同一文件夹下的 `update-settings.json` 中。

<a id="replace-keys"></a>

## 更换密钥

如果你分享过备份，请生成新的密钥和新的管理员密码。已保存的令牌仍然可用，所有人都会被退出登录。

打开 **Tools → Backup → Replace keys**，输入新的管理员密码并确认。

<a id="logs"></a>

## 日志
管理员可以打开 Settings 旁边的 **Logs**（日志），筛选并实时查看服务器、请求、智能体运行和数据库日志。每次运行都有一个 **View logs** 链接。

**Export logs** 会把符合当前筛选条件的日志行保存为 `coppice-logs.zip`。Coppice 会移除它已知的令牌和密钥，但分享之前请自己再检查一遍。

**Report a problem** 会新建一个 GitHub issue，并自动填好你的 Coppice 版本。写明发生了什么、你原本期望什么，然后附上通过 **Export logs** 导出的 zip 文件。

<h2 id="logs-1"><code>[logs]</code></h2>

| 字段 | 类型 | 默认值 | 作用 |
| --- | --- | --- | --- |
| `retention_days` | 整数 | `14` | 日志保留天数（含当天）：3、7、14 或 30，默认 14。Coppice 每小时删除一次过期日志，修改无需重启即可生效。如果日志超过 256 MiB，即使还没到 `retention_days`，Coppice 也会从最旧的开始删除。 |

即使超过限制，Coppice 也不会删除当天的日志，以及正在写入的数据库日志。数据库日志计入 256 MiB 上限，超过 `retention_days` 后会被删除。

| 字段 | 类型 | 默认值 | 作用 |
| --- | --- | --- | --- |
| `dir` | 字符串 | `./data/logs` | 日志文件夹。桌面应用使用自己的文件夹，会忽略此项。修改后需要重启。 |

<a id="connectors"></a>

## 连接器
已登录的 CLI 默认开启。只有想关闭某个连接器时，才需要加一行：

```toml
[agent.connectors.claude-code]
enabled = false
```

**Tools → Connectors** 中的开关写入的也是这一行。

Kilo Code 默认关闭，需要你手动开启。Coppice 无法免费检查它的登录状态。

## `[agent]`

| 字段 | 类型 | 默认值 | 作用 |
| --- | --- | --- | --- |
| `worktrees_path` | 字符串 | `<data-dir>/worktrees` | 存放 worktree 的文件夹，每个工单各有一个 git worktree。 |
| `worker_count` | 整数 | `2` | 同时运行的智能体任务数。小于 1 时按 1 处理。 |
| `health_check_interval_secs` | 整数 | `60` | 智能体健康检查的间隔秒数。小于 10 时按 10 处理。 |

`<data-dir>` 是存放 `config.toml` 的文件夹：Linux 和 WSL 上为 `~/.config/Coppice`，macOS 上为 `~/Library/Application Support/Coppice`。

## `[agent.connectors.<id>]`

可用的 id：`claude-code`、`codex`、`cursor`、`opencode`、`kilo-code`。每个表都是可选的。

| 字段 | 类型 | 默认值 | 作用 |
| --- | --- | --- | --- |
| `enabled` | 布尔值 | 未设置：CLI 已登录则开启 | 开启或关闭该连接器。 |
| `model_providers` | 字符串列表 | `[]` | 该连接器的模型提供商。开启连接器时自动填写。 |
| `run_timeout_secs` | 整数 | OpenCode 为 `1800`，其他为 `600` | 该连接器上的一次运行超过多少秒算失败。 |
| `command` | 字符串 | `agent`（Cursor）、`codex`、`opencode`、`kilo` | 要运行的 CLI 可执行文件。Claude Code 始终运行 `claude`，没有 `command`。 |

仅限 OpenCode：

| 字段 | 类型 | 默认值 | 作用 |
| --- | --- | --- | --- |
| `serve_hostname` | 字符串 | `127.0.0.1` | 运行期间 OpenCode 辅助进程使用的主机名。修改后需要重启。 |

修改 OpenCode 的 `command` 也需要重启。

## `[workflow]`

| 字段 | 类型 | 默认值 | 作用 |
| --- | --- | --- | --- |
| `auto_start_runs` | 布尔值 | `false` | 为 true 时，给已关联仓库的工单指派智能体会自动开始运行。为 false 时，需要你点击 **Run agent**。 |
| `auto_assign.default` | 布尔值 | `true` | 为所有没有单独设置的状态开启自动指派。 |
| `auto_assign.<status>` | 布尔值 | `backlog`：`false`；其他：使用 `default` | 为某个状态开启或关闭自动指派。 |
| `auto_split.default` | 布尔值 | `false` | 为所有没有单独设置的状态开启自动拆分。 |
| `auto_split.<status>` | 布尔值 | 使用 `default` | 为某个状态开启或关闭自动拆分。 |

状态键：`backlog`、`ready`、`in_progress`、`in_review`、`in_qa`、`wait_for_final_review`（Wait for Human Review）、`blocked`、`done`。

某个状态开启自动指派时，Coppice 会指派该次运行指定的智能体。关闭时，工单上会改为显示 **Recommends:**（推荐）。

某个状态开启自动拆分时，Coppice 会创建该次运行建议的子工单。关闭时，它会保存一条 **Pending split recommendation**（待处理的拆分建议），直到你点击 **Approve splits**。

## `[knowledge]`

| 字段 | 类型 | 默认值 | 作用 |
| --- | --- | --- | --- |
| `enabled` | 布尔值 | `true` | 开启或关闭知识压缩整理。 |
| `poll_interval_ms` | 整数 | `500` | 压缩调度器的唤醒频率。小于 100 时按 100 处理。 |
| `compaction.interval_secs` | 整数 | `1800` | 两轮压缩之间的间隔秒数。 |
| `compaction.batch_max_tickets` | 整数 | `10` | 一批最多包含的工单数。 |
| `compaction.batch_max_source_bytes` | 整数 | `200000` | 一批最多包含的源数据字节数。 |
| `compaction.max_candidates_per_batch` | 整数 | `20` | 一批最多能提出的候选条目数。 |
| `compaction.max_attempts` | 整数 | `3` | 一批最多尝试的次数。 |
| `auto_save.enabled` | 布尔值 | `false` | 不经询问，直接保存低风险的候选条目。 |
| `auto_save.allowed_types` | 字符串列表 | `[]` | 自动保存允许保存的类型：`bug_pattern`、`coding_convention`、`dependency_note`、`performance_note`、`review_feedback`、`test_command`。 |
| `auto_save.minimum_confidence` | 字符串 | `high` | 必须保持为 `high`。 |
| `retrieval.top_k` | 整数 | `8` | 一次查询返回的条目数，1 到 20。 |
| `retrieval.allowed_types` | 字符串列表 | `[]`（所有类型） | 只返回这些类型：`coding_convention`、`architecture_rule`、`bug_pattern`、`test_command`、`review_feedback`、`dependency_note`、`api_contract`、`workflow_rule`、`human_preference`、`operational_runbook`、`security_rule`、`performance_note`。 |
| `retrieval.minimum_confidence` | 字符串 | `medium` | `low`、`medium` 或 `high`。 |
| `retrieval.default_page_size` | 整数 | `25` | 默认每页条数。 |
| `retrieval.max_page_size` | 整数 | `100` | 每页最大条数，最多 100。 |
| `retrieval.max_active_per_board` | 整数 | `1000` | 一块看板最多可容纳的 Knowledge 条目数（待批准或已批准）。该看板上智能体的条目也计入在内。 |
| `retrieval.max_active_workspace` | 整数 | `10000` | 适用于整个工作区的 Knowledge 条目上限（待批准或已批准）。 |

以上所有数量和间隔都必须大于 0。

## `[git]`

| 字段 | 类型 | 默认值 | 作用 |
| --- | --- | --- | --- |
| `author_name` | 字符串 | `Coppice` | Coppice 所做提交的作者名。 |
| `author_email` | 字符串 | `coppice@localhost` | 这些提交的作者邮箱。 |
| `push_enabled` | 布尔值 | `false` | 允许 Coppice 推送分支并创建 pull request。 |

## `[plugins]`

| 字段 | 类型 | 默认值 | 作用 |
| --- | --- | --- | --- |
| `dir` | 字符串 | `<data-dir>/plugins` | 你的默认插件文件夹。更多文件夹可在 Plugins 页面添加。 |
| `allow_file_git_urls` | 布尔值 | `false` | 允许安装插件时使用 `file:` 形式的 git URL。 |
| `git_timeout_secs` | 整数 | `300` | 插件克隆或更新超过多少秒算失败。 |
| `mcp_start_timeout_secs` | 整数 | `20` | 等待插件 MCP 服务器启动的秒数。 |
| `mcp_list_timeout_secs` | 整数 | `10` | 等待插件列出工具的秒数。 |
| `mcp_idle_shutdown_secs` | 整数 | `600` | 空闲的插件 MCP 服务器保持运行的秒数。 |

## `[mcp]`

| 字段 | 类型 | 默认值 | 作用 |
| --- | --- | --- | --- |
| `builtin_plugins_dir` | 字符串 | `<data-dir>/builtin-plugins` | Coppice 每次启动时写入内置技能的位置。在这里做的修改会丢失。 |
| `call_timeout_secs` | 整数 | `60` | 单次工具调用超过多少秒算失败。 |
| `max_output_bytes` | 整数 | `32768` | 工具结果的最大字节数。 |
| `comment_post_limit` | 整数 | `5` | 一次运行最多能发表的评论数。 |
| `token_ttl_secs` | 整数 | `14400` | 一次运行的工具令牌有效期（秒）。 |

## `[keybindings]`

每个键都是[键盘快捷键](/docs/shortcuts)中的一个命令 id。你设置的按键会替换该命令的默认按键，设为空值则关闭该快捷键。

| 字段 | 类型 | 默认值 | 作用 |
| --- | --- | --- | --- |
| `"<command id>"` | 字符串或字符串列表 | 内置快捷键 | 替换该命令的按键。`""` 会移除它们。 |

```toml
[keybindings]
"palette.open" = "P"
"ticket.create" = ""
"board.nextCard" = ["J", "Down"]
```

`Mod` 在 macOS 上是 Cmd，在 Linux 和 WSL 上是 Ctrl。按键序列写作 `G then B`。一个命令的多个按键用 ` or ` 分隔，或写成列表。保存后无需重启即可生效。无法解析的快捷键、未知的命令 id、已被其他命令占用的按键，以及属于浏览器或系统的按键都会被跳过。该命令保留默认按键，Settings 会说明原因。
