import { describe, expect, it } from 'vitest';
import type { UsageCoverage } from '../../lib/schemas/connector';
import {
  OTHER_KEY,
  chartSeries,
  coverageLine,
  dayStack,
  filledDays,
  inputCount,
  rangeDays,
  rowCanReport,
  rowLabel,
} from './usageSummaryModel';

const name = (id: string) => ({ 'claude-code': 'Claude Code', cursor: 'Cursor' })[id] ?? id;

const coverage = (fields: Partial<UsageCoverage['fields']>): UsageCoverage => ({
  granularity: 'turn',
  model: 'runOnly',
  fields: { input: true, cacheRead: true, cacheWrite: true, output: true, reasoning: true, ...fields },
  subagents: 'none',
  contextWindow: 'none',
});

describe('rangeDays', () => {
  it('counts today as the last day of the range', () => {
    const today = new Date(2026, 10, 10);
    expect(rangeDays('7', today)).toEqual({ from: '2026-11-04', to: '2026-11-10' });
    expect(rangeDays('all', today)).toEqual({ from: null, to: null });
  });
});

describe('rowLabel', () => {
  it('names keys the server leaves unnamed', () => {
    expect(rowLabel('board', { key: 'workspace', label: null }, name)).toBe('Workspace');
    expect(rowLabel('model', { key: 'cursor/', label: null }, name)).toBe('Model not reported');
    expect(rowLabel('connector', { key: 'cursor', label: null }, name)).toBe('Cursor');
  });
});

describe('coverageLine', () => {
  it('summarizes tracked vs untracked runs', () => {
    expect(
      coverageLine(
        {
          trackedRuns: 42,
          untrackedRuns: 18,
          untrackedByConnector: [{ connector: 'cursor', runs: 12 }],
        },
        name,
      ),
    ).toContain('42 of 60');
    expect(
      coverageLine({ trackedRuns: 0, untrackedRuns: 0, untrackedByConnector: [] }, name),
    ).toContain('No runs');
  });
});

describe('rowCanReport', () => {
  it('says "never" only for one connector whose coverage leaves the count out', () => {
    const coverageOf = (id: string) => (id === 'turny' ? coverage({ cacheRead: false }) : coverage({}));
    const row = { key: 'turny/m', connector: 'turny' };
    expect(rowCanReport('model', row, 'cacheRead', coverageOf)).toBe(false);
    expect(rowCanReport('model', row, 'output', coverageOf)).toBe(true);
    // Agent rows can mix connectors: never "—".
    expect(rowCanReport('agent', { key: 'a', connector: null }, 'cacheRead', coverageOf)).toBe(true);
    // Unknown coverage: assume it could report.
    expect(rowCanReport('model', row, 'cacheRead', () => undefined)).toBe(true);
  });
});

describe('chart series', () => {
  it('folds groups past the seventh into Other and fills empty days', () => {
    const rows = Array.from({ length: 9 }, (_, i) => ({
      key: `k${i}`,
      label: null,
      connector: null,
      tokens: { input: null, cacheRead: null, cacheWrite: null, output: null, reasoning: null },
      unsplit: 0,
      total: 100 - i,
      runs: 1,
    }));
    const { keys, hasOther } = chartSeries(rows);
    expect(keys).toHaveLength(7);
    expect(hasOther).toBe(true);
    const stack = dayStack({ k0: 5, k7: 3, k8: 2 }, keys, hasOther);
    expect(stack[0]).toEqual({ key: 'k0', value: 5 });
    expect(stack[stack.length - 1]).toEqual({ key: OTHER_KEY, value: 5 });

    const days = filledDays(
      [
        { day: '2026-11-01', byKey: { a: 1 } },
        { day: '2026-11-03', byKey: { a: 2 } },
      ],
      null,
      null,
    );
    expect(days.map((d) => d.day)).toEqual(['2026-11-01', '2026-11-02', '2026-11-03']);
    expect(days[1].byKey).toEqual({});
  });
});

describe('inputCount', () => {
  it('adds cache writes to uncached input, like the Runs tab', () => {
    expect(inputCount({ input: 10, cacheRead: 99, cacheWrite: 5, output: 1, reasoning: null })).toBe(15);
    expect(
      inputCount({ input: null, cacheRead: 1, cacheWrite: null, output: 1, reasoning: null }),
    ).toBeNull();
  });
});
