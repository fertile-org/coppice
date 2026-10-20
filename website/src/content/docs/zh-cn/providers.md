---
title: 提供商
description: "Coppice 运行的智能体 CLI：Claude Code、Codex、Cursor、OpenCode、Kilo Code、GitHub Copilot、Qwen Code、Kimi Code、Grok 和 Grok。"
---

# 提供商

受够了为每个智能体都开一个终端？Coppice 运行你已经在用的编程智能体 CLI，并把它们的工作放到同一块看板上。订阅和登录都还是你自己的，不需要 Coppice 账号。

<a id="supported-connectors"></a>

## 支持的连接器

| 连接器 | Coppice 查找的命令 | 安装 |
| --- | --- | --- |
| Claude Code | `claude` | [安装指南](https://docs.anthropic.com/en/docs/claude-code/setup) |
| Codex | `codex` | [安装指南](https://developers.openai.com/codex/cli) |
| Cursor | `cursor-agent` | [安装指南](https://cursor.com/docs/cli/installation) |
| OpenCode | `opencode` | [安装指南](https://opencode.ai/docs/) |
| Kilo Code | `kilo` | [安装指南](https://kilo.ai/docs/cli) |
| GitHub Copilot | `copilot` | [安装指南](https://docs.github.com/en/copilot/how-tos/copilot-cli) |
| Qwen Code | `qwen` | [安装指南](https://github.com/QwenLM/qwen-code) |
| Kimi Code | `kimi` | [安装指南](https://moonshotai.github.io/kimi-code/) |
| Grok | `grok` | [安装指南](https://x.ai/cli) |

<figure>
<img src="/assets/docs/model-picker.webp" width="1440" height="900" alt="智能体上的模型选择器，列出该智能体 CLI 提供的模型。">
<figcaption>选择一个模型，或保留默认值。</figcaption>
</figure>

为这个智能体选择一个模型，或保留默认值。装了新模型？点击 Refresh models（刷新模型）。

<a id="connect-your-agent-cli"></a>

## 连接你的智能体 CLI

1. 像平常一样安装 CLI，并在终端里登录。
2. 创建智能体时选择你的智能体 CLI，Coppice 就会启用它。

智能体表单会列出每个连接器及其状态：

| 状态 | 含义 |
| --- | --- |
| **Ready** | 一次免费的登录检查确认你已登录。 |
| **Found, not signed in** | Coppice 找到了命令，但你还没有登录。请在终端里登录。 |
| **Not on your PATH** | Coppice 找不到该命令。请安装它，或检查你的 PATH。 |
| **Installed** | Coppice 找到了命令。对于 Kilo Code、GitHub Copilot、Qwen Code、Kimi Code、Grok 和 Grok，不消耗你的套餐额度就无法检查登录状态。 |

Kilo Code、GitHub Copilot、Qwen Code、Kimi Code、Grok 和 Grok 显示为 **Installed**。System → Connectors 中还会提示："Coppice can't check this sign-in without using your plan. A run will show an error if you're signed out."（不消耗你的套餐额度，Coppice 就无法检查登录状态；如果你已退出登录，运行时会报错。）想确认它能用，就点 **Test connection**（测试连接）：它会跑一次简短的聊天、规划和工单。所有 CLI 都能聊天。Claude Code、Codex、Cursor 和 Kilo Code 的聊天会显示 "Coppice hasn't confirmed that {cli} stays read-only. The agent can still run commands on your computer."（Coppice 尚未确认 {cli} 会保持只读，智能体仍可在你的电脑上执行命令。）OpenCode、GitHub Copilot、Qwen Code、Kimi Code、Grok 和 Grok 的聊天没有这条横幅：在聊天和规划中，Coppice 会阻止它们写文件和执行 shell 命令。你在这些 CLI 里自己配置的 MCP 工具仍可能改动内容。

CLI 还没就绪时，也可以先保存智能体。CLI 安装并登录后，它就会开始处理工单。如果你是在打开 Coppice 之后才安装的 CLI，请退出并重新打开 Coppice，让它找到新命令。

要关闭某个连接器，请使用 **System → Connectors**（系统 → 连接器）。

<a id="use-your-own-api-key-or-another-provider"></a>

## 使用自己的 API 密钥或其他提供商

想用自己的 API 密钥，或者想用你的 CLI 不提供的模型？请用 OpenCode。它支持 75 个以上的模型提供商，也支持本地模型。参见 [OpenCode 的提供商列表](https://opencode.ai/docs/providers/)。

1. 安装 OpenCode。
2. 在终端运行 `opencode`，输入 `/connect`，选择你的提供商并粘贴密钥。密钥由 OpenCode 保管，Coppice 从不索要。列表里没有？请按 OpenCode 的[自定义提供商步骤](https://opencode.ai/docs/providers/#custom-provider)操作。
3. 在 Coppice 中创建一个智能体，把 **Connector**（连接器）设为 **OpenCode**。你的提供商的模型会出现在 **Model**（模型）下。如果你是在 Coppice 打开期间添加的提供商，请点击 **Refresh models**（刷新模型）。

<a id="on-a-ticket"></a>

## 在工单上

智能体接手工单时，Coppice 会在该工单自己的 git worktree 中启动它的 CLI。你可以在工单上实时查看进度。

<a id="what-coppice-does-not-do"></a>

## Coppice 不做什么
- 不提供云端编码服务
- 不会在未经你 Human Review（人工审核）的情况下合并代码
