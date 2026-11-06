import type { TokenCounts } from '../../lib/schemas/usage';

export const USAGE_COPY = {
  notReported: 'Tokens not reported',
  partial: 'The CLI has not reported its own total yet; this is the sum of the requests so far.',
  noUsage: 'No token usage recorded for this run.',
} as const;

/** `950`, `1.2k`, `182k`, `3.4M`. */
export function formatTokens(n: number): string {
  const abs = Math.abs(n);
  if (abs < 1000) return String(n);
  const [value, unit] = abs < 1_000_000 ? [n / 1000, 'k'] : [n / 1_000_000, 'M'];
  const digits = Math.abs(value) < 10 ? 1 : 0;
  return `${value.toFixed(digits).replace(/\.0$/, '')}${unit}`;
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
export function tokenDetail(tokens: TokenCounts): string {
  const fmt = (n: number | null) => (n == null ? 'not reported' : n.toLocaleString());
  return [
    `Input: ${fmt(tokens.input)}`,
    `Cache write: ${fmt(tokens.cacheWrite)}`,
    `Cache read: ${fmt(tokens.cacheRead)}`,
    `Output: ${fmt(tokens.output)}`,
    `Reasoning (part of output): ${fmt(tokens.reasoning)}`,
  ].join('\n');
}
