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

/** Search phrase the site leads with, ahead of the CLI names. Matches the homepage title. */
export const PRIMARY_KEYWORD = 'multi-agent coding';

/** Second search phrase: what the app looks like once it is open. */
export const SECONDARY_KEYWORD = 'AI coding agent board';
