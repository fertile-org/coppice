import { describe, expect, it } from 'vitest';
import {
  normalizeCommentMarkdown,
  splitMarkdownFrontmatter,
} from './TicketMarkdown';

describe('splitMarkdownFrontmatter', () => {
  it('extracts name and description from a SKILL.md frontmatter block', () => {
    const raw = [
      '---',
      'name: coppice-qc-verification',
      'description: QC rules for verifying in_qa tickets',
      '---',
      '',
      '# Coppice platform rules — QA verification',
      '',
      'Body text.',
    ].join('\n');
    const { fields, body } = splitMarkdownFrontmatter(raw);
    expect(fields).toEqual([
      { key: 'name', value: 'coppice-qc-verification' },
      { key: 'description', value: 'QC rules for verifying in_qa tickets' },
    ]);
    expect(body).toContain('# Coppice platform rules — QA verification');
    expect(body).not.toContain('name:');
  });

  it('leaves content unchanged when there is no frontmatter', () => {
    const raw = '# Title\n\nHello.';
    expect(splitMarkdownFrontmatter(raw)).toEqual({ fields: null, body: raw });
  });

  it('leaves content unchanged when the opening --- is unclosed', () => {
    const raw = '---\nname: orphan\n\n# Still body';
    expect(splitMarkdownFrontmatter(raw)).toEqual({ fields: null, body: raw });
  });

  it('strips surrounding quotes from frontmatter values', () => {
    const raw = ['---', 'name: "quoted"', "title: 'also'", '---', 'Body'].join('\n');
    expect(splitMarkdownFrontmatter(raw).fields).toEqual([
      { key: 'name', value: 'quoted' },
      { key: 'title', value: 'also' },
    ]);
  });
});

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
