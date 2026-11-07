export const locale = 'en';

/** English homepage and download copy. A locale file in this folder overrides any keys it sets. */
export const pages = {
  home: {
    title: 'Coppice: AI Coding Agent Board, Local and Open Source',
    description:
      'Coppice is an AI coding agent board on your own machine. Claude Code, Codex, Cursor, and others plan, build, and review in parallel. You accept every diff.',
    worksWith: 'Works with',
    h1: 'Hire specialists. Keep the final say.',
    lede: 'Your PM agent drafts the plan, and you approve it. Specialist agents build in parallel. Nothing is done until you accept the diff.',
    hero: {
      stillAlt: 'Coppice board with tickets for the acme-web repository',
      videoAlt:
        'Coppice: write a ticket, compare design options, approve the plan a PM agent drafts, let agents build and test, then comment on the diff. Nothing is done until you accept it.',
      captions: [
        'Write a ticket.',
        'Your PM agent drafts a plan.',
        'It shows you design options.',
        'You approve it.',
        'Agents build, review, and test.',
        'Review the diff. Comment on any line.',
        'Nothing is done until you accept it.',
      ],
    },
    support: "Open source (Apache 2.0). It runs on your machine, and you don't need a Coppice account.",
    downloads: {
      mac: 'Download for macOS (Apple Silicon)',
      linux: 'Download for Linux (.deb)',
      docs: 'Read the docs',
    },
    howTitle: 'How it works',
    steps: [
      {
        title: 'Hand off the work.',
        body: 'Write a ticket, or approve one an agent suggests. Assign it to your PM agent. It drafts a plan and shows you design options for UI changes. You approve the plan before coding starts, unless you choose Skip planning.',
      },
      {
        title: 'Let agents build.',
        body: 'Each ticket gets its own git worktree, so parallel agents never touch your main checkout. Follow any run live.',
      },
      {
        title: 'Human Review.',
        body: 'Read the diff in the app and comment on any line. Nothing counts as done until you accept it.',
      },
    ],
    howClose: "Download the app and open it. There's nothing to set up: no Docker and no Coppice account.",
    whyTitle: 'Why Coppice',
    reasons: [
      {
        title: 'The right model for each role.',
        body: 'Each agent uses its own CLI and model. You can plan and review with your strongest model, and code with a faster one, on the same board.',
      },
      {
        title: 'Stop tab-hopping between terminals.',
        body: "Each agent's work is a card on one board. Blocked tickets, and work waiting for your review, stand out, so you can see where you're needed.",
      },
      {
        title: 'Catch mistakes before they land.',
        body: 'Every change waits for you as a diff in the app. Nothing counts as done until you accept it, and what you accept is exactly what merges.',
      },
      {
        title: 'No second subscription.',
        body: 'Coppice drives the CLIs you already pay for: Claude Code, Codex, Cursor, OpenCode, Kilo Code, GitHub Copilot, Qwen Code, Kimi Code, and Grok.',
      },
    ],
    notTitle: 'What it is not',
    notItems: [
      'Not a cloud coding service',
      'Not a Jira replacement',
      'Not “tell the agent and get the finished product”',
    ],
  },
  download: {
    title: 'Download Coppice',
    metaTitle: 'Download Coppice for macOS and Linux',
    description:
      'Download Coppice, the free, open source AI coding agent board, for macOS (Apple Silicon or Intel) and Linux. It runs on your machine without a Coppice account.',
    intro: "Coppice is free. It runs on your machine, and you don't need a Coppice account.",
    groups: [
      {
        note: 'Not sure which Mac you have? Open Apple menu → About This Mac. If it lists a Chip, choose Apple Silicon. If it lists a Processor, choose Intel.',
        rows: [
          { name: 'macOS (Apple Silicon)', label: 'Download .dmg' },
          { name: 'macOS (Intel)', label: 'Download .dmg' },
        ],
      },
      {
        rows: [
          { name: 'Linux (x64)', label: 'Download .deb' },
          { name: 'Linux (ARM64)', label: 'Download .deb' },
        ],
      },
      {
        rows: [{ name: 'Windows 11', label: 'Install through WSL' }],
      },
    ],
    checksum: {
      before: 'Check your download against ',
      sumsLabel: 'SHA256SUMS',
      between: '. See ',
      installLabel: 'Install',
      after: ' for how.',
    },
    next: {
      before: 'Next, follow the ',
      label: 'install steps',
      after: '.',
    },
  },
};

export type Pages = typeof pages;
