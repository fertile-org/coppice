export const locale = 'en';

/** English homepage and download copy. A locale file in this folder overrides any keys it sets. */
export const pages = {
  home: {
    title: 'Coppice: desktop app for multi-agent coding',
    description:
      'Coppice is a desktop app for multi-agent coding. You write tickets, approve the plan, and accept the diff. Agents run CLIs such as Claude Code and Codex.',
    worksWith: 'Works with',
    h1: 'Hire specialists. Keep the final say.',
    lede: 'A desktop app for multi-agent coding. Your PM agent drafts the plan, and you approve it. Specialist agents build in parallel. Nothing is done until you accept the diff.',
    hero: {
      stillAlt: 'Coppice board with tickets for the acme-web repository',
      videoAlt:
        'Coppice: write a ticket, compare design options, approve the plan a PM agent drafts, let agents build and test, then comment on the diff. Nothing is done until you accept it.',
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
        title: 'Planning with the PM agent',
        body: 'Your PM agent reads the ticket and drafts a plan: the approach, the steps, and how the change will be checked. You approve that plan before any code is written.',
        imageAlt:
          'Plan Review for a dark mode toggle. The PM agent is assigned, and the proposed plan lists the approach and the steps.',
      },
      {
        title: 'Approve the design',
        body: 'When the ticket changes what people see, the agent shows a few layouts and marks the one it recommends. You choose one, and the plan follows that choice.',
        imageAlt:
          'Design options for a Settings theme control, with three layouts and a preview of the recommended switch.',
      },
      {
        title: 'Let agents build',
        body: 'Specialist agents build, review, and test the ticket. Each ticket gets its own git worktree, so parallel work stays off your main checkout. You can follow a run while it happens.',
        imageAlt: 'The board, with tickets in progress, in review, in QA, and waiting for a person.',
      },
      {
        title: 'Review the code changes',
        body: 'Read the diff in the app and comment on any line. Nothing counts as done until you accept it. Accept and Merge stay separate choices.',
        imageAlt:
          'Code review for a theme toggle: the approved plan, the changed file, and a comment on one line of the diff.',
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
      'Download Coppice, the free, open source desktop app for multi-agent coding, for macOS and Linux. It runs on your machine without a Coppice account.',
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
    'Coppice is a desktop app for multi-agent coding. You write tickets on your own machine. The agents run CLIs you already use, such as Claude Code, Codex, and Cursor, and each ticket gets its own git worktree. You don\'t need a Coppice account. Coppice does not ask for or store a provider API key.',
  learnBefore: 'When you want the first ticket, start with ',
  learnLabel: 'Getting started',
  learnAfter: '.',
};
