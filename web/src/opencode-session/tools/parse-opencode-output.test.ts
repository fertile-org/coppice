import { describe, expect, it } from 'vitest';
import {
  excerptWebContent,
  isMostlyHtml,
  parseFileToolOutput,
  stripLineNumberPrefixes,
  stripSystemReminders,
} from './parse-opencode-output';

describe('parseFileToolOutput', () => {
  it('parses OpenCode read XML and strips line numbers', () => {
    const raw = `<path>/tmp/foo.md</path>
<type>file</type>
<content>
1: # Title
2: 
3: body
</content>`;

    const parsed = parseFileToolOutput(raw);
    expect(parsed?.path).toBe('/tmp/foo.md');
    expect(parsed?.content).toBe('# Title\n\nbody');
  });

  it('removes system-reminder blocks', () => {
    const raw = `<path>/tmp/a.md</path><type>file</type><content>1: hi</content>
<system-reminder>secret</system-reminder>`;

    expect(parseFileToolOutput(raw)?.content).toBe('hi');
  });
});

describe('stripLineNumberPrefixes', () => {
  it('strips numbered prefixes', () => {
    expect(stripLineNumberPrefixes('1: a\n12: b')).toBe('a\nb');
  });
});

describe('stripSystemReminders', () => {
  it('removes reminder tags', () => {
    expect(stripSystemReminders('before<system-reminder>x</system-reminder>after')).toBe('beforeafter');
  });
});

describe('web fetch excerpt', () => {
  it('detects HTML', () => {
    expect(isMostlyHtml('<html><body><div><p>hi</p></div></body></html>')).toBe(true);
  });

  it('excerpts HTML to plain text', () => {
    const excerpt = excerptWebContent('<html><body><h1>Title</h1><p>Hello world</p></body></html>');
    expect(excerpt).toContain('Title');
    expect(excerpt).toContain('Hello world');
    expect(excerpt).not.toContain('<html');
  });
});
