import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { resolveLoginShellPath } from '../src/shellPath.mjs';

test('extracts PATH between markers from noisy shell output', async () => {
  let seen;
  const run = async (argv) => {
    seen = argv;
    return 'Welcome!\n__COPPICE_PATH__/a:/b__COPPICE_PATH__\nbye';
  };
  assert.equal(await resolveLoginShellPath({ shell: '/bin/zsh', run }), '/a:/b');
  assert.deepEqual(seen, [
    '/bin/zsh',
    '-ilc',
    "printf '__COPPICE_PATH__%s__COPPICE_PATH__' \"$PATH\"",
  ]);
});

test('falls back to current PATH plus common dirs, de-duplicated', async () => {
  const run = async () => {
    throw new Error('boom');
  };
  const result = await resolveLoginShellPath({ shell: '/bin/zsh', run });
  const entries = result.split(path.delimiter);
  assert.equal(entries.filter((e) => e === '/usr/local/bin').length, 1);
  assert.ok(entries.includes('/opt/homebrew/bin'));
  for (const entry of new Set((process.env.PATH ?? '').split(path.delimiter).filter(Boolean))) {
    assert.equal(entries.filter((e) => e === entry).length, 1, entry);
  }
});

test('falls back when markers are missing', async () => {
  const result = await resolveLoginShellPath({ shell: '/bin/zsh', run: async () => 'nothing' });
  assert.ok(result.split(path.delimiter).includes('/usr/local/bin'));
});

test('default runner returns the real login shell PATH', async () => {
  const result = await resolveLoginShellPath({ shell: '/bin/sh' });
  assert.ok(result.length > 0);
  assert.ok(!result.includes('__COPPICE_PATH__'));
});

test('falls back when the shell hangs past the timeout', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'coppice-shell-test-'));
  const shell = path.join(dir, 'slow-shell');
  fs.writeFileSync(shell, '#!/bin/sh\nsleep 5\n', { mode: 0o755 });
  const started = Date.now();
  const result = await resolveLoginShellPath({ shell, timeoutMs: 100 });
  assert.ok(Date.now() - started < 2000);
  assert.ok(result.split(path.delimiter).includes('/usr/local/bin'));
});
