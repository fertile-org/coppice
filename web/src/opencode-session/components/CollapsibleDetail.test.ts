import { describe, expect, it } from 'vitest';
import { excerptPreview } from './CollapsibleDetail';

describe('excerptPreview', () => {
  it('collapses whitespace and truncates long text', () => {
    expect(excerptPreview('hello\n\nworld')).toBe('hello world');
    expect(excerptPreview('x'.repeat(120))).toHaveLength(101);
    expect(excerptPreview('x'.repeat(120)).endsWith('…')).toBe(true);
  });
});
