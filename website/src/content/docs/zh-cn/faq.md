---
title: 常见问题
description: "Beta 范围、后续计划，以及 Coppice 不是什么。"
---

# 常见问题

<a id="is-coppice-production-ready"></a>

## Coppice 可以用于生产环境了吗？
还不行。Coppice 正处于 Beta 阶段，面向想要一块本地智能体看板、并且真正由人把关的早期用户。智能体以你的本地权限运行。

<a id="do-i-need-docker-or-an-account"></a>

## 需要 Docker 或账号吗？
不需要。下载桌面应用，打开就能用。不需要 Docker，不需要配置服务器，也不需要 Coppice 账号。

<a id="is-this-a-jira--linear-replacement"></a>

## 它能替代 Jira / Linear 吗？
不能。Coppice 是一块用来驱动编程智能体的看板，不是面向整个公司的完整问题跟踪系统。

<a id="is-this-a-cloud-coding-agent"></a>

## 这是云端编程智能体吗？
不是。智能体通过你电脑上的 CLI 在你的仓库里运行。两类产品的区别在于：云端智能体服务把工作交给别人的电脑去做；Coppice 让工作留在本地，并由你把关。

<a id="will-agents-merge-without-me"></a>

## 智能体会不经过我就合并吗？
不会。Human Review（人工审核）是 Coppice 的核心关卡。你打开 diff，由你决定。

<a id="what-happens-if-a-tickets-changes-conflict-with-main"></a>

## 如果工单的改动和 main 冲突怎么办？
Coppice 会在合并前停下，不动 main。它会列出冲突的文件，并把工单送回 In Progress，由同一个智能体开始解决冲突。之后你需要在 Human Review 中再接受一次，才会合并。

<a id="what-about-set-and-forget-autopilots"></a>

## “设好就不用管”的自动驾驶呢？
那不是 Coppice 要做的产品。Coppice 是为管理智能体而打造的：工作由你发起，每个改动都由你接受。

<a id="windows"></a>

## Windows 呢？
Windows 11 应该可以通过 WSL 运行 Linux x64 版本，但我们还没有测试过。参见 [Windows 11（通过 WSL）](/docs/install#windows-11-through-wsl)。如果你试了，欢迎[在 GitHub 上告诉我们结果](https://github.com/fertile-org/coppice/issues)。本次 Beta 没有计划推出原生 Windows 应用。

<a id="where-do-contributors-look"></a>

## 贡献者该看哪里？
架构、开发和测试文档都在 [GitHub 仓库](https://github.com/fertile-org/coppice/tree/main/docs)中。本站讲的是如何使用 Coppice。
