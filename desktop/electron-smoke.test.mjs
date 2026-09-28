import { spawn } from 'node:child_process';
import { once } from 'node:events';
import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

test('electron main module loads', async () => {
  const electronPath = path.join(__dirname, 'node_modules', 'electron', 'cli.js');
  const child = spawn(process.execPath, [electronPath, '--version'], {
    cwd: __dirname,
    env: { ...process.env, ELECTRON_DISABLE_SANDBOX: '1' },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  const [code] = await once(child, 'close');
  assert.equal(code, 0);
});
