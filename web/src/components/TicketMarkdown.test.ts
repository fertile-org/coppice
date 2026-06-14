import { describe, expect, it } from 'vitest';
import { normalizeCommentMarkdown } from './TicketMarkdown';

describe('normalizeCommentMarkdown', () => {
  it('inserts a blank line before bold section labels', () => {
    const raw = 'Approving.\n**Tests run:**\n- cargo test';
    expect(normalizeCommentMarkdown(raw)).toBe(
      'Approving.\n\n**Tests run:**\n- cargo test',
    );
  });

  it('leaves already well-spaced markdown unchanged', () => {
    const raw = 'Summary line.\n\n**Changed files:**\n- a.rs';
    expect(normalizeCommentMarkdown(raw)).toBe(raw);
  });
});
