import { describe, expect, it } from 'vitest';
import { fromUtcRfc3339, logsPath, toUtcRfc3339 } from './query';

describe('log query', () => {
  it('sends a clock time as UTC', () => {
    expect(toUtcRfc3339('2026-10-14T03:35')).toBe('2026-10-14T03:35:00Z');
    expect(fromUtcRfc3339('2026-10-14T03:35:00Z')).toBe('2026-10-14T03:35');
  });

  it('builds the read path from the current filters', () => {
    expect(
      logsPath(
        {
          source: 'cli',
          level: 'info',
          from: '2026-10-14T03:35:00Z',
          to: '',
          run: 'run-9',
          q: 'health',
        },
        'cursor-1',
      ),
    ).toBe(
      '/api/logs?source=cli&level=info&from=2026-10-14T03%3A35%3A00Z&run=run-9&q=health&cursor=cursor-1',
    );
  });
});
