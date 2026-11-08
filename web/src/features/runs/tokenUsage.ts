import type { TokenCounts } from '../../lib/schemas/usage';

export const USAGE_COPY = {
  notReported: 'Tokens not reported',
  /** A running run: the CLI reports its own total when it ends. */
  partialLive: 'Sum of the requests so far. The CLI reports its own total when the run ends.',
  /** A finished run that ended without the CLI's total. */
  partialEnded:
    'The CLI did not report its own total, so this is what the run streamed: each request, plus subagents counted by their own totals.',
  noUsage: 'No token usage recorded for this run.',
  nothingYet: 'No tokens reported yet.',
} as const;

/** "1 run ended without …" / "2 runs ended without …". */
export function partialRunsNote(n: number): string {
  return `${n} ${n === 1 ? 'run' : 'runs'} ended before the CLI reported a total; ${
    n === 1 ? 'it counts' : 'they count'
  } what was streamed.`;
}

/** One decimal below 10, none above. Rounds on integers: `toFixed` misrounds 9.95. */
function scaled(abs: number, scale: number): number {
  const tenths = Math.round((abs * 10) / scale);
  return tenths < 100 ? tenths / 10 : Math.round(abs / scale);
}

/** `950`, `1.2k`, `182k`, `3.5M`. Switches to M after rounding, so 999,500 is `1M`. */
export function formatTokens(n: number): string {
  const abs = Math.abs(n);
  if (abs < 1000) return String(n);
  const sign = n < 0 ? '-' : '';
  const thousands = scaled(abs, 1000);
  if (thousands < 1000) return `${sign}${thousands}k`;
  return `${sign}${scaled(abs, 1_000_000)}M`;
}

function sum(...parts: (number | null)[]): number | null {
  return parts.some((p) => p != null)
    ? parts.reduce<number>((total, p) => total + (p ?? 0), 0)
    : null;
}

/** Field-by-field sum; a field stays null only when both sides lack it. */
export function addTokens(a: TokenCounts, b: TokenCounts): TokenCounts {
  return {
    input: sum(a.input, b.input),
    cacheRead: sum(a.cacheRead, b.cacheRead),
    cacheWrite: sum(a.cacheWrite, b.cacheWrite),
    output: sum(a.output, b.output),
    reasoning: sum(a.reasoning, b.reasoning),
  };
}

/** New input: uncached plus what was written to the cache. */
export function newInputTokens(tokens: TokenCounts): number | null {
  return sum(tokens.input, tokens.cacheWrite);
}

/** Every token processed, cached reads included. */
export function totalTokens(tokens: TokenCounts): number | null {
  return sum(tokens.input, tokens.cacheWrite, tokens.cacheRead, tokens.output);
}

/**
 * `24k in · 182k cached · 9k out`. Cached reads are kept apart from new input:
 * they dominate the count but cost far less against plan limits.
 * Parts the CLI did not report are left out; `null` when it reported none.
 */
export function tokenSummary(tokens: TokenCounts): string | null {
  const parts: string[] = [];
  const input = newInputTokens(tokens);
  if (input != null) parts.push(`${formatTokens(input)} in`);
  if (tokens.cacheRead != null) parts.push(`${formatTokens(tokens.cacheRead)} cached`);
  if (tokens.output != null) parts.push(`${formatTokens(tokens.output)} out`);
  return parts.length > 0 ? parts.join(' · ') : null;
}

/** Exact numbers for a tooltip. */
export function tokenDetail(tokens: TokenCounts, subagentTokens: number | null = null): string {
  const fmt = (n: number | null) => (n == null ? 'not reported' : n.toLocaleString());
  const lines = [
    `Input: ${fmt(tokens.input)}`,
    `Cache write: ${fmt(tokens.cacheWrite)}`,
    `Cache read: ${fmt(tokens.cacheRead)}`,
    `Output: ${fmt(tokens.output)}`,
    `Reasoning (part of output): ${fmt(tokens.reasoning)}`,
  ];
  if (subagentTokens != null) {
    lines.push(`Subagents, no breakdown: ${fmt(subagentTokens)}`);
  }
  return lines.join('\n');
}

/**
 * Usage as the server reports it: the breakdown, plus subagent tokens that came
 * without one (partial totals only).
 */
export interface UsageAmount {
  tokens: TokenCounts;
  subagentTokens: number | null;
}

/** Every token, subagents without a breakdown included. */
export function usageTotal({ tokens, subagentTokens }: UsageAmount): number | null {
  return sum(totalTokens(tokens), subagentTokens);
}

/** `tokenSummary`, plus `· 200k subagents` for subagents without a breakdown. */
export function usageSummary({ tokens, subagentTokens }: UsageAmount): string | null {
  const parts = [tokenSummary(tokens)];
  if (subagentTokens != null) parts.push(`${formatTokens(subagentTokens)} subagents`);
  const shown = parts.filter((part): part is string => part != null);
  return shown.length > 0 ? shown.join(' · ') : null;
}

/** Field-by-field sum of two amounts. */
export function addUsage(a: UsageAmount, b: UsageAmount): UsageAmount {
  return {
    tokens: addTokens(a.tokens, b.tokens),
    subagentTokens: sum(a.subagentTokens, b.subagentTokens),
  };
}
