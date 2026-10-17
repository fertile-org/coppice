import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { OLD_DOMAIN, checkOldDomain, findInBuffer } from '../scripts/check-old-domain.mjs';

const script = fileURLToPath(new URL('../scripts/check-old-domain.mjs', import.meta.url));

function run(args, cwd) {
  return spawnSync(process.execPath, [script, ...args], { cwd, encoding: 'utf8' });
}

test('the old host matches with any case and the new host does not', () => {
  assert.deepEqual(findInBuffer(Buffer.from(`a\nhttps://apt.${OLD_DOMAIN}/x\n`), 'f'), ['f:2']);
  assert.deepEqual(findInBuffer(Buffer.from(OLD_DOMAIN.toUpperCase()), 'f'), ['f:1']);
  assert.deepEqual(findInBuffer(Buffer.from('https://apt.getcoppice.app getcoppice.vercel.app'), 'f'), []);
});

test('packaged files, updater-mode.json and the CNAME fail on the old host', () => {
  const dir = mkdtempSync(join(tmpdir(), 'coppice-old-domain-'));
  try {
    const app = join(dir, 'linux-unpacked', 'resources');
    mkdirSync(app, { recursive: true });
    writeFileSync(join(app, 'app.asar'), Buffer.concat([Buffer.from([0, 1, 2]), Buffer.from(`apt.${OLD_DOMAIN}`)]));
    writeFileSync(join(dir, 'updater-mode.json'), `{"updateBase":"https://apt.getcoppice.app"}\n`);
    writeFileSync(join(dir, 'CNAME'), `${OLD_DOMAIN}\n`);

    const bundle = checkOldDomain({ paths: [join(dir, 'linux-unpacked')], root: dir });
    assert.deepEqual(bundle.hits, [join('linux-unpacked', 'resources', 'app.asar') + ':1']);
    assert.equal(checkOldDomain({ paths: [join(dir, 'updater-mode.json')], root: dir }).hits.length, 0);
    assert.equal(run([join(dir, 'CNAME')], dir).status, 1);
    assert.equal(run([join(dir, 'updater-mode.json')], dir).status, 0);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('the check fails closed when it cannot read its input', () => {
  const dir = mkdtempSync(join(tmpdir(), 'coppice-old-domain-closed-'));
  try {
    const missing = run([join(dir, 'missing.json')], dir);
    assert.equal(missing.status, 1);
    assert.match(missing.stderr, /could not run/);
    assert.equal(run([], dir).status, 1);
    // Not a git checkout: --source can't list files.
    assert.equal(run(['--source'], dir).status, 1);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('--source reads tracked files and fails on a tracked match', () => {
  const dir = mkdtempSync(join(tmpdir(), 'coppice-old-domain-git-'));
  try {
    const git = (...args) => spawnSync('git', args, { cwd: dir, encoding: 'utf8' });
    assert.equal(git('init', '-q').status, 0);
    writeFileSync(join(dir, 'ok.md'), 'https://getcoppice.app\n');
    git('add', 'ok.md');
    assert.equal(run(['--source'], dir).status, 0);
    writeFileSync(join(dir, 'bad.md'), `line\nsee ${OLD_DOMAIN}\n`);
    git('add', 'bad.md');
    const bad = run(['--source'], dir);
    assert.equal(bad.status, 1);
    assert.match(bad.stderr, /bad\.md:2/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('the repository has no old-domain references', () => {
  const root = fileURLToPath(new URL('../..', import.meta.url));
  assert.deepEqual(checkOldDomain({ source: true, root }).hits, []);
});
