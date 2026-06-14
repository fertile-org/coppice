import { describe, expect, it } from 'vitest';
import { formatReviewPreview } from './formatReviewPreview';

describe('formatReviewPreview', () => {
  it('groups inline comments by file', () => {
    const body = formatReviewPreview(
      'coppice',
      '/data/worktrees/TICKET-abc-coppice',
      'main',
      'agent/TICKET-abc',
      'abc1234',
      'Looks good overall.',
      [
        { path: 'src/a.ts', line: 10, side: 'new', body: 'Fix this' },
        { path: 'src/b.rs', line: 3, side: 'old', body: 'Rename' },
      ],
    );

    expect(body).toContain('### Summary\nLooks good overall.');
    expect(body).toContain('#### `src/a.ts`');
    expect(body).toContain('**L10** (new): Fix this');
    expect(body).toContain('#### `src/b.rs`');
  });
});
