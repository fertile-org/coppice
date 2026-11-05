import { describe, expect, it } from 'vitest';
import {
  nextUnviewed,
  readViewedPaths,
  stepFile,
  viewedStorageKey,
  writeViewedPaths,
} from './reviewNavigation';

describe('review file navigation', () => {
  it('stays put at the ends and wraps only to an unviewed file', () => {
    const paths = ['a', 'b', 'c'];
    expect(stepFile(paths, 'a', -1)).toBe('a');
    expect(stepFile(paths, 'c', 1)).toBe('c');
    expect(stepFile(paths, 'b', 1)).toBe('c');

    const viewed = new Set(['a']);
    expect(nextUnviewed(paths, viewed, 'a')).toBe('b');
    expect(nextUnviewed(paths, new Set(['a', 'b', 'c']), 'b')).toBe('b');
    expect(nextUnviewed(paths, new Set(['b', 'c']), 'a')).toBe('a');
  });

  it('keeps viewed paths for one head SHA', () => {
    const storage = localStorage;
    const first = viewedStorageKey('repo', '/wt', 'aaa');
    const second = viewedStorageKey('repo', '/wt', 'bbb');
    writeViewedPaths(storage, first, ['src/a.ts']);
    expect(readViewedPaths(storage, first)).toEqual(['src/a.ts']);
    expect(readViewedPaths(storage, second)).toEqual([]);
  });
});
