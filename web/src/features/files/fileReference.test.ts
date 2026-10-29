import { describe, expect, it } from 'vitest';
import {
  findProseFileReferences,
  parseFileReference,
  ticketFileUrl,
} from './fileReference';

describe('parseFileReference', () => {
  it('parses code and image paths with optional line ranges', () => {
    expect(parseFileReference('server/src/lib.rs')).toEqual({
      path: 'server/src/lib.rs',
      kind: 'text',
    });
    expect(parseFileReference('./web/src/App.tsx:42')).toEqual({
      path: 'web/src/App.tsx',
      kind: 'text',
      line: 42,
    });
    expect(parseFileReference('docs/a.md#L3-L9')).toEqual({
      path: 'docs/a.md',
      kind: 'text',
      line: 3,
      endLine: 9,
    });
    expect(parseFileReference('docs/shot.PNG')).toEqual({ path: 'docs/shot.PNG', kind: 'image' });
    expect(parseFileReference('Makefile')?.kind).toBe('text');
  });

  it('rejects unsupported or unsafe paths', () => {
    for (const raw of [
      'build/app.exe',
      '/etc/passwd.txt',
      '../secret.txt',
      '.git/config.toml',
      'https://example.com/a.png',
      'cargo test --lib',
      'v1.2',
    ]) {
      expect(parseFileReference(raw), raw).toBeNull();
    }
  });
});

describe('findProseFileReferences', () => {
  it('finds paths with a directory and trims sentence punctuation', () => {
    const text = 'Changed server/src/api/mod.rs:12 and web/src/App.tsx. Uses Node.js.';
    expect(findProseFileReferences(text).map((m) => m.raw)).toEqual([
      'server/src/api/mod.rs:12',
      'web/src/App.tsx',
    ]);
  });

  it('ignores URLs, directories and unsupported files', () => {
    const text = 'See https://x.dev/docs/a.md, the src/features/ folder and bin/tool.exe';
    expect(findProseFileReferences(text)).toEqual([]);
  });
});

describe('ticketFileUrl', () => {
  it('encodes the path', () => {
    expect(ticketFileUrl('t1', 'src/a b.rs')).toBe(
      '/api/tickets/t1/files/content?path=src%2Fa%20b.rs',
    );
  });
});
