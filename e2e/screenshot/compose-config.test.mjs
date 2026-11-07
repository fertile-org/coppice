import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { describe, it } from 'node:test';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

function runCheck() {
  return spawnSync('node', ['e2e/screenshot/check-compose.mjs'], {
    cwd: root,
    encoding: 'utf8',
    env: process.env,
  });
}

describe('screenshot compose config', () => {
  it('keeps the bind mount for a local engine and publishes no host file in copy mode', () => {
    const result = runCheck();
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
    assert.match(result.stdout, /127\.0\.0\.1 only/);
    assert.match(result.stdout, /copy mode publishes no host file/);
  });
});
