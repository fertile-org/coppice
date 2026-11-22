import { describe, expect, it } from 'vitest';
import {
  excerptWebContent,
  isMostlyHtml,
  parseFileToolOutput,
  stripLineNumberPrefixes,
  stripSystemReminders,
} from './parse-opencode-output';

describe('parseFileToolOutput', () => {
  it('parses OpenCode read XML, strips line numbers, and drops reminders', () => {
    const raw = `<path>/tmp/foo.md</path>
<type>file</type>
<content>
1: # Title
2: 
3: body
</content>`;
    expect(parseFileToolOutput(raw)?.content).toBe('# Title\n\nbody');

    const withReminder = `<path>/tmp/a.md</path><type>file</type><content>1: hi</content>
<system-reminder>secret</system-reminder>`;
    expect(parseFileToolOutput(withReminder)?.content).toBe('hi');
  });
});

describe('web fetch excerpt', () => {
  it('detects HTML and excerpts it', () => {
    const html = '<html><body><h1>Title</h1><p>Hello world</p></body></html>';
    expect(isMostlyHtml(html)).toBe(true);
    const excerpt = excerptWebContent(html);
    expect(excerpt).toContain('Title');
    expect(excerpt).not.toContain('<html');
  });

  it('strips line numbers and system reminders', () => {
    expect(stripLineNumberPrefixes('1: a\n12: b')).toBe('a\nb');
    expect(stripSystemReminders('before<system-reminder>x</system-reminder>after')).toBe('beforeafter');
  });
});
