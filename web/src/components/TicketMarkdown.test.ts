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

  it('strips an empty agent-requests marker line alone', () => {
    expect(
      normalizeCommentMarkdown('<!-- coppice-agent-requests: [] -->'),
    ).toBe('');
  });

  it('strips an empty agent-requests marker after a summary', () => {
    const raw =
      'Done with the fix.\n\n<!-- coppice-agent-requests: [] -->';
    expect(normalizeCommentMarkdown(raw)).toBe('Done with the fix.');
  });

  it('strips a non-empty agent-requests marker and keeps consultation section', () => {
    const raw = [
      'Need input.',
      '',
      '**Consultation requests:**',
      '- @backend_engineer: Confirm API contract',
      '',
      '<!-- coppice-agent-requests: [{"agentKey":"backend_engineer","intent":"consult","request":"Confirm API contract"}] -->',
    ].join('\n');
    expect(normalizeCommentMarkdown(raw)).toBe(
      [
        'Need input.',
        '',
        '**Consultation requests:**',
        '- @backend_engineer: Confirm API contract',
      ].join('\n'),
    );
  });

  it('strips marker adjacent to normal summary markdown', () => {
    const raw = [
      'Approving.',
      '**Tests run:**',
      '- make web-test',
      '',
      '<!-- coppice-agent-requests: [] -->',
    ].join('\n');
    expect(normalizeCommentMarkdown(raw)).toBe(
      'Approving.\n\n**Tests run:**\n- make web-test',
    );
  });
});
