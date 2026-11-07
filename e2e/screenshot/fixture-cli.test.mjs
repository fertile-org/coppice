import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { describe, it } from 'node:test';

const here = path.dirname(fileURLToPath(import.meta.url));

function runFixture(name, args) {
  return spawnSync(path.join(here, 'fixtures/bin', name), args, {
    encoding: 'utf8',
    env: {
      PATH: '/usr/bin:/bin',
      HOME: '/home/host-should-not-appear',
      ANTHROPIC_API_KEY: 'should-not-appear',
      OPENAI_API_KEY: 'should-not-appear',
    },
  });
}

describe('demo connector fixtures', () => {
  it('answers the Claude Code sign-in probe without a host secret', () => {
    const version = runFixture('claude', ['--version']);
    assert.equal(version.status, 0, version.stderr);
    assert.match(version.stdout, /^claude 2\./);
    const status = runFixture('claude', ['auth', 'status']);
    assert.equal(status.status, 0, status.stderr);
    assert.match(status.stdout, /"loggedIn":true/);
    assert.doesNotMatch(`${status.stdout}${status.stderr}`, /should-not-appear|host-should-not-appear/);
  });

  it('answers the Codex sign-in probe with an empty success', () => {
    const version = runFixture('codex', ['--version']);
    assert.equal(version.status, 0, version.stderr);
    assert.match(version.stdout, /^codex-cli /);
    const status = runFixture('codex', ['login', 'status']);
    assert.equal(status.status, 0, status.stderr);
    assert.equal(status.stdout, '');
    assert.doesNotMatch(`${status.stdout}${status.stderr}`, /should-not-appear|host-should-not-appear/);
  });
});
