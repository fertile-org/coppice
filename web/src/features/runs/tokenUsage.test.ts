import { describe, expect, it } from 'vitest';
import type { TokenCounts } from '../../lib/schemas/usage';
import {
  addTokens,
  addUsage,
  formatTokens,
  partialRunsNote,
  tokenDetail,
  tokenSummary,
  totalTokens,
  usageSummary,
  usageTotal,
} from './tokenUsage';

const none: TokenCounts = {
  input: null,
  cacheRead: null,
  cacheWrite: null,
  output: null,
  reasoning: null,
};

describe('formatTokens', () => {
  it('compacts thousands and millions', () => {
    expect(formatTokens(950)).toBe('950');
    expect(formatTokens(1000)).toBe('1k');
    expect(formatTokens(1234)).toBe('1.2k');
    expect(formatTokens(182_400)).toBe('182k');
    expect(formatTokens(3_450_000)).toBe('3.5M');
    expect(formatTokens(12_000_000)).toBe('12M');
  });

  it('rounds before choosing the unit', () => {
    expect(formatTokens(999_500)).toBe('1M');
    expect(formatTokens(999_499)).toBe('999k');
    expect(formatTokens(9_950)).toBe('10k');
    expect(formatTokens(-999_500)).toBe('-1M');
  });
});

describe('tokenSummary', () => {
  it('keeps cached reads apart from new input', () => {
    expect(
      tokenSummary({ ...none, input: 4, cacheWrite: 24_000, cacheRead: 182_000, output: 9_100 }),
    ).toBe('24k in · 182k cached · 9.1k out');
  });

  it('leaves out what was not reported', () => {
    expect(tokenSummary({ ...none, output: 12 })).toBe('12 out');
    expect(tokenSummary(none)).toBeNull();
  });
});

describe('totals', () => {
  it('adds every processed token and keeps missing fields null', () => {
    expect(totalTokens(none)).toBeNull();
    expect(totalTokens({ ...none, input: 1, cacheRead: 10, output: 2, reasoning: 1 })).toBe(13);
    expect(addTokens({ ...none, input: 1 }, { ...none, input: 2, output: 3 })).toEqual({
      ...none,
      input: 3,
      output: 3,
    });
  });
});

describe('usage with subagents that have no breakdown', () => {
  const tokens = { ...none, input: 1_000, output: 500 };

  it('adds them to the total and the summary', () => {
    expect(usageTotal({ tokens, subagentTokens: 200_000 })).toBe(201_500);
    expect(usageSummary({ tokens, subagentTokens: 200_000 })).toBe('1k in · 500 out · 200k subagents');
    expect(usageTotal({ tokens, subagentTokens: null })).toBe(1_500);
    expect(usageSummary({ tokens: none, subagentTokens: null })).toBeNull();
    expect(usageSummary({ tokens: none, subagentTokens: 3 })).toBe('3 subagents');
  });

  it('adds amounts and lists them in the tooltip', () => {
    expect(
      addUsage({ tokens, subagentTokens: null }, { tokens: none, subagentTokens: 7 }),
    ).toEqual({ tokens, subagentTokens: 7 });
    expect(tokenDetail(tokens, 7)).toContain('Subagents, no breakdown: 7');
    expect(tokenDetail(tokens)).not.toContain('Subagents');
  });

  it('words the partial-runs note by count', () => {
    expect(partialRunsNote(1)).toBe(
      '1 run ended before the CLI reported a total; it counts what was streamed.',
    );
    expect(partialRunsNote(2)).toContain('2 runs ended');
  });
});
