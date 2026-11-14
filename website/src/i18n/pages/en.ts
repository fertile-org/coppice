export const locale = 'en';

/** English homepage and download copy. A locale file in this folder overrides any keys it sets. */
export const pages = {
  home: {
    title: 'Coppice: desktop app for multi-agent coding',
    description:
      'Coppice is a desktop app for multi-agent coding. You write tickets, approve the plan, and accept the diff. Agents run CLIs such as Claude Code and Codex.',
    worksWith: 'Works with',
    h1: 'Hire specialists. Keep the final say.',
    lede: 'A desktop app for multi-agent coding with Claude Code, Codex, and more. Your PM agent drafts the plan, and you approve it. Specialist agents build in parallel. Nothing is done until you accept the diff.',
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
        body: 'Each ticket gets its own git worktree, so parallel agents never touch your main checkout. Once you start a ticket, agents can continue automatically through the workflow. Follow any run live.',
      },
      {
        title: 'Human Review.',
        body: 'Read the diff in the app and comment on any line. Nothing counts as done until you accept it. Accept and Merge are separate choices.',
      },
    ],
    howClose:
      "Download the app and open it. You don't need Docker or a Coppice account. You do need a local git checkout and one supported agent CLI, signed in with that CLI.",
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
        body: 'You review the final diff in the app. Nothing counts as done until you accept it. Accept and Merge are separate choices.',
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
          { name: 'macOS (Apple Silicon) · arm64', label: 'Download .dmg' },
          { name: 'macOS (Intel) · x64', label: 'Download .dmg' },
        ],
      },
      {
        rows: [
          { name: 'Linux (x64) · amd64', label: 'Download .deb' },
          { name: 'Linux (ARM64)', label: 'Download .deb' },
        ],
      },
      {
        rows: [{ name: 'Windows 11 · x64 through WSL', label: 'Install through WSL' }],
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

/**
 * English-only homepage explanation. These strings are plain text.
 * HomePage renders links itself; Markdown such as **bold** is not interpreted.
 */
export const homeEntry = {
  whatTitle: 'What Coppice is',
  whatBody:
    'Coppice is a local desktop board for coding agents. You write tickets on your own machine. The agents run CLIs you already use, such as Claude Code, Codex, and Cursor, and each ticket gets its own git worktree. You don\'t need a Coppice account. Coppice does not ask for or store a provider API key.',
  guidesTitle: 'Guides',
  guides: [
    {
      href: '/docs/install',
      label: 'Install',
      note: 'Packages by platform and architecture, with links to the setup steps.',
    },
    {
      href: '/docs/getting-started',
      label: 'Getting started',
      note: 'What to install, how to sign in, and the first ticket.',
    },
    {
      href: '/docs/providers',
      label: 'Providers',
      note: 'Supported agent CLIs, and which sign-ins Coppice can check.',
    },
    {
      href: '/docs/concepts',
      label: 'Concepts',
      note: 'How a ticket moves, including Accept and Merge.',
    },
    {
      href: '/docs/faq',
      label: 'FAQ',
      note: 'Beta limits, Docker, Windows, and what Coppice is not.',
    },
  ],
};
