import type { Pages } from './en';

export const locale = 'zh-cn';

/** 简体中文 homepage and download copy. Keys mirror `./en.ts`; anything missing stays English. */
export const pages: Pages = {
  home: {
    description:
      '各有分工的编程智能体在同一块看板上规划、构建和审查，你接受 diff 之前，什么都不算完成。本地运行，开源，用的是你已经付费的智能体 CLI。',
    h1: '雇用专家，你来拍板。',
    lede: '你的 PM 智能体起草计划，由你批准。专业智能体并行构建。你接受 diff 之前，什么都不算完成。',
    hero: {
      stillAlt: 'Coppice 看板，上面是 acme-web 仓库的工单',
      videoAlt:
        'Coppice：写一个工单，批准 PM 智能体起草的计划，让智能体构建和测试，然后对 diff 发表评论。你接受之前，什么都不算完成。',
      captions: [
        '写一个工单。',
        '你的 PM 智能体起草计划。',
        '你来批准。',
        '智能体构建、审查、测试。',
        '审核 diff，可以评论任意一行。',
        '你接受之前，什么都不算完成。',
      ],
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
        title: '把工作交出去。',
        body: '写一个工单，或批准智能体建议的工单。把它指派给你的 PM 智能体，它会起草一份计划，经你批准后才会有人写代码。',
      },
      {
        title: '让智能体去构建。',
        body: '每个工单都有自己的 git worktree，并行的智能体永远不会碰你的主检出目录。任何一次运行都可以实时查看。',
      },
      {
        title: '人工审核。',
        body: '在应用里阅读 diff，可以评论任意一行，你接受之前，什么都不算完成。',
      },
    ],
    howClose: '下载、打开、开干。无需任何配置：不用 Docker，也不用 Coppice 账号。',
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
        body: 'Coppice 驱动的是你已经付费的 CLI：Claude Code、Codex、Cursor、OpenCode、Kilo Code、GitHub Copilot、Qwen Code、Kimi Code、Grok 和 Grok。',
      },
    ],
    notTitle: '它不是什么',
    notItems: ['不是云端编程服务', '不是 Jira 的替代品', '不是“告诉智能体一句，就拿到成品”'],
  },
  download: {
    title: '下载 Coppice',
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
      before: '接下来，请按',
      label: '安装步骤',
      after: '操作。',
    },
  },
};
