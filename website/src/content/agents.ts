/**
 * Agent CLIs Coppice runs, in the order of the Providers doc. Product names, not translated.
 * Feeds keywords and structured data; keep in step with `src/pages/docs/providers.md`.
 */
export const AGENT_CLIS = [
  'Claude Code',
  'Codex',
  'Cursor',
  'OpenCode',
  'Kilo Code',
  'GitHub Copilot',
  'Qwen Code',
  'Kimi Code',
  'Grok',
] as const;

/** Search phrase the site leads with, ahead of the CLI names. */
export const PRIMARY_KEYWORD = 'AI coding agent board';
