/** Tailwind classes mapped to OpenCode CLI semantic colors (see opencode-theme.css). */
export const sessionTheme = {
  root: 'oc-session',

  text: 'text-[var(--oc-text)]',
  textMuted: 'text-[var(--oc-text-muted)]',
  textDim: 'text-[var(--oc-text-dim)]',
  textPrimary: 'text-[var(--oc-primary)]',

  border: 'border-[var(--oc-border-subtle)]',
  borderStrong: 'border-[var(--oc-border)]',

  bg: 'bg-[var(--oc-bg)]',
  bgPanel: 'bg-[var(--oc-bg-panel)]',
  bgElement: 'bg-[var(--oc-bg-element)]',
  bgCode: 'bg-[var(--oc-bg-code)]',

  accent: 'text-[var(--oc-primary)]',
  secondary: 'text-[var(--oc-secondary)]',
  success: 'text-[var(--oc-success)]',
  warning: 'text-[var(--oc-warning)]',
  error: 'text-[var(--oc-error)]',
  info: 'text-[var(--oc-info)]',
  thinking: 'text-[var(--oc-thinking)]',
  thinkingDetail: 'text-[var(--oc-thinking-detail)]',
  mode: 'text-[var(--oc-mode)]',

  toolActive: 'text-[var(--oc-tool-active)]',
  toolComplete: 'text-[var(--oc-tool-complete)]',

  sectionGap: 'gap-[var(--oc-gap-section)]',
  sectionCard: 'border border-[var(--oc-border)] bg-[var(--oc-bg-panel)] px-3 py-2.5',
  outputBlock: 'oc-output-block',

  fontBody: 'font-mono text-[length:var(--oc-font-size)] leading-[var(--oc-line-height)]',
  fontMono: 'font-mono text-[length:var(--oc-font-size)] leading-[var(--oc-line-height)]',
  fontMonoSm: 'font-mono text-[length:var(--oc-font-size-sm)] leading-[var(--oc-line-height)]',
  fontMonoXs: 'font-mono text-[length:var(--oc-font-size-xs)] leading-[var(--oc-line-height)]',

  markdownHeading: 'text-[var(--oc-markdown-heading)]',
  markdownLink: 'text-[var(--oc-markdown-link)]',
  markdownCode: 'text-[var(--oc-markdown-code)]',
  markdownStrong: 'text-[var(--oc-markdown-strong)]',
  markdownEmph: 'text-[var(--oc-markdown-emph)]',
  markdownBlockquote: 'text-[var(--oc-markdown-blockquote)]',
} as const;
