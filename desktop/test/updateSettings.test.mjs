import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { readUpdateSettings, writeUpdateSettings } from '../src/updateSettings.mjs';

test('automatic updates default on when the file is missing or invalid', () => {
  const dir = mkdtempSync(join(tmpdir(), 'coppice-update-settings-'));
  const file = join(dir, 'update-settings.json');
  assert.deepEqual(readUpdateSettings(file), { automaticUpdates: true });

  const broken = join(dir, 'broken.json');
  writeFileSync(broken, '{');
  assert.deepEqual(readUpdateSettings(broken), { automaticUpdates: true });
});

test('automatic updates persists across reads', () => {
  const dir = mkdtempSync(join(tmpdir(), 'coppice-update-settings-'));
  const file = join(dir, 'nested', 'update-settings.json');
  assert.deepEqual(writeUpdateSettings(file, { automaticUpdates: false }), {
    automaticUpdates: false,
  });
  assert.deepEqual(readUpdateSettings(file), { automaticUpdates: false });
  assert.equal(readFileSync(file, 'utf8').includes('"automaticUpdates":false'), true);

  writeUpdateSettings(file, { automaticUpdates: true });
  assert.deepEqual(readUpdateSettings(file), { automaticUpdates: true });
});
