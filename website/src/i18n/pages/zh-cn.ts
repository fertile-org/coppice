import type { Pages } from './en';

export const locale = 'zh-cn';

/** 简体中文 homepage and download copy. Keys mirror `./en.ts`; anything missing stays English. */
export const pages: Pages = {
  home: {
    title: 'Coppice：多智能体编程的桌面应用',
    description:
      'Coppice 是用于多智能体编程的桌面应用。你写工单、批准计划，并接受 diff。Claude Code、Codex 等智能体在你的电脑上运行。',
    worksWith: '支持',
    h1: '雇用专家，你来拍板。',
    lede: '这是用于多智能体编程的桌面应用。你的 PM 智能体起草计划，由你批准。专业智能体并行构建。你接受 diff 之前，什么都不算完成。',
    hero: {
      stillAlt: 'Coppice 看板，上面是 acme-web 仓库的工单',
      videoAlt:
        'Coppice：写一个工单，比较设计方案，批准 PM 智能体起草的计划，让智能体构建和测试，然后对 diff 发表评论。你接受之前，什么都不算完成。',
    },
    support: '开源（Apache 2.0）。本地运行，无需 Coppice 账号。',
    downloads: {
      mac: '下载 macOS 版（Apple Silicon）',
      linux: '下载 Linux 版（.deb）',
      docs: '阅读文档',
    },
    howTitle: '工作方式',
    steps: [
      {
        title: '和 PM 智能体一起规划',
        body: 'PM 智能体阅读工单并起草计划：做法、步骤，以及如何检查这次改动。开始写代码之前，你先批准这份计划。',
        imageAlt: '深色模式开关的计划审核。PM 智能体是负责人，提议的计划列出了做法和步骤。',
      },
      {
        title: '批准设计',
        body: '当工单会改变人们看到的界面时，智能体会给出几个布局，并标出它推荐的一个。你选定一个，计划就按这个选择来写。',
        imageAlt: 'Settings 主题控件的设计方案，有三种布局，以及推荐开关的预览。',
      },
      {
        title: '让智能体去构建',
        body: '专业智能体负责构建、审查和测试。每个工单都有自己的 git worktree，并行的工作不会碰到你的主检出目录。运行过程中你可以跟着看。',
        imageAlt: '看板，上面有进行中、审查中、测试中，以及等你处理的工单。',
      },
      {
        title: '审查代码改动',
        body: '在应用里阅读 diff，可以评论任意一行。你接受之前，什么都不算完成。接受和合并是两件分开的事。',
        imageAlt: '主题开关的代码审查：已批准的计划、改动的文件，以及 diff 某一行上的评论。',
      },
    ],
    whyTitle: '为什么选择 Coppice',
    reasons: [
      {
        title: '每个角色都用合适的模型。',
        body: '每个智能体各自选择 CLI 和模型。用最强的模型做规划和审查，用更快的模型写代码，全都在同一块看板上。',
      },
      {
        title: '不用再在终端之间来回切换。',
        body: '每个智能体的工作都是同一块看板上的一张卡片。受阻的工单和等你审核的工作一目了然，你知道哪里需要你。',
      },
      {
        title: '在落地之前发现错误。',
        body: '每个改动都会以 diff 的形式在应用里等你审核。你接受之前，什么都不算完成；你接受的内容，就是最终合并的内容。',
      },
      {
        title: '不用再多一份订阅。',
        body: 'Coppice 驱动的是你已经付费的 CLI：Claude Code、Codex、Cursor、OpenCode、Kilo Code、GitHub Copilot、Qwen Code、Kimi Code 和 Grok。',
      },
    ],
    notTitle: '它不是什么',
    notItems: ['不是云端编程服务', '不是 Jira 的替代品', '不是“告诉智能体一句，就拿到成品”'],
  },
  download: {
    title: '下载 Coppice',
    metaTitle: '下载 Coppice（macOS 和 Linux）',
    description:
      '下载 Coppice：免费开源的多智能体编程桌面应用，支持 macOS（Apple Silicon 或 Intel）和 Linux。本地运行，无需 Coppice 账号。',
    intro: '免费，在你自己的电脑上运行，无需 Coppice 账号。',
    groups: [
      {
        note: '不确定你的 Mac 是哪种？打开苹果菜单 → 关于本机。如果显示“芯片”，请选择 Apple Silicon；如果显示“处理器”，请选择 Intel。',
        rows: [
          { name: 'macOS (Apple Silicon)', label: '下载 .dmg' },
          { name: 'macOS (Intel)', label: '下载 .dmg' },
        ],
      },
      {
        rows: [
          { name: 'Linux (x64)', label: '下载 .deb' },
          { name: 'Linux (ARM64)', label: '下载 .deb' },
        ],
      },
      {
        rows: [{ name: 'Windows 11', label: '通过 WSL 安装' }],
      },
    ],
    checksum: {
      before: '可以用 ',
      sumsLabel: 'SHA256SUMS',
      between: ' 校验你下载的文件，具体方法见',
      installLabel: '安装',
      after: '文档。',
    },
    next: {
      before: '接下来，跟着',
      label: '安装步骤',
      after: '做。',
    },
  },
};
