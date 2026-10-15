import { describe, expect, it } from 'vitest';
import { reportProblemBody } from './copy';
import {
  exportPayload,
  filenameFromDisposition,
  fromUtcRfc3339,
  logsPath,
  reportProblemUrl,
  toUtcRfc3339,
} from './query';

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

  it('sends the current filters and the desktop version', () => {
    expect(
      exportPayload(
        {
          source: 'cli',
          level: 'warn',
          from: '',
          to: '',
          run: 'run-9',
          q: '',
        },
        '1.4.2',
      ),
    ).toEqual({
      level: 'warn',
      source: 'cli',
      run: 'run-9',
      shellVersion: '1.4.2',
    });
  });

  it('reads the saved file name', () => {
    expect(filenameFromDisposition('attachment; filename="coppice-logs.zip"')).toBe(
      'coppice-logs.zip',
    );
    expect(filenameFromDisposition(null)).toBe('coppice-logs.zip');
  });

  it('opens a GitHub issue with the four-line body', () => {
    const url = new URL(reportProblemUrl('0.1.0', 'Debian GNU/Linux 12'));
    expect(url.origin + url.pathname).toBe('https://github.com/fertile-org/coppice/issues/new');
    expect(url.searchParams.get('body')).toBe(reportProblemBody('0.1.0', 'Debian GNU/Linux 12'));
    expect(url.searchParams.get('body')).toBe(
      [
        'Coppice 0.1.0 on Debian GNU/Linux 12',
        'What happened:',
        'What you expected:',
        'Attach the file from Logs → Export logs.',
      ].join('\n'),
    );
  });
});
