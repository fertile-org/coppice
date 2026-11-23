import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { appendUpdaterLog, updaterLogPath } from '../src/updaterLog.mjs';

test('appendUpdaterLog writes updater events under the app logs directory', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'coppice-updater-log-'));
  try {
    appendUpdaterLog(dir, 'checking-for-update');
    appendUpdaterLog(dir, 'error: offline');
    const body = fs.readFileSync(updaterLogPath(dir), 'utf8');
    assert.match(body, /checking-for-update/);
    assert.match(body, /error: offline/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
