import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { OLD_DOMAIN, checkOldDomain, findInBuffer } from '../scripts/check-old-domain.mjs';

const script = fileURLToPath(new URL('../scripts/check-old-domain.mjs', import.meta.url));

function run(args, cwd) {
  return spawnSync(process.execPath, [script, ...args], { cwd, encoding: 'utf8' });
}

function initGit(dir) {
  const git = (...args) => {
    const result = spawnSync('git', args, { cwd: dir, encoding: 'utf8' });
    assert.equal(result.status, 0, `${args.join(' ')}: ${result.stderr || result.stdout}`);
    return result;
  };
  git('init', '-q');
  return git;
}

test('the old host matches with any case and the new host does not', () => {
  assert.deepEqual(findInBuffer(Buffer.from(`a\nhttps://apt.${OLD_DOMAIN}/x\n`), 'f'), ['f:2']);
  assert.deepEqual(findInBuffer(Buffer.from(OLD_DOMAIN.toUpperCase()), 'f'), ['f:1']);
  assert.deepEqual(findInBuffer(Buffer.from('https://apt.getcoppice.app https://getcoppice.app'), 'f'), []);
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

test('a tracked symlink to a directory passes without EISDIR', () => {
  const dir = mkdtempSync(join(tmpdir(), 'coppice-old-domain-dirlink-'));
  try {
    const git = initGit(dir);
    const skills = join(dir, '.agents', 'skills');
    mkdirSync(skills, { recursive: true });
    writeFileSync(join(skills, 'SKILL.md'), 'https://getcoppice.app\n');
    mkdirSync(join(dir, '.claude'));
    symlinkSync(join('..', '.agents', 'skills'), join(dir, '.claude', 'skills'));
    git('add', '-A');
    // Not tracked. Reading the directory through the symlink would see it.
    writeFileSync(join(skills, 'local.md'), `${OLD_DOMAIN}\n`);

    assert.deepEqual(checkOldDomain({ source: true, root: dir }).hits, []);
    const result = run(['--source'], dir);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stderr, '');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a tracked symlink fails when its link text contains the old domain', () => {
  const dir = mkdtempSync(join(tmpdir(), 'coppice-old-domain-linktext-'));
  try {
    const git = initGit(dir);
    const name = `note-${OLD_DOMAIN}.txt`;
    writeFileSync(join(dir, 'clean.txt'), 'https://getcoppice.app\n');
    writeFileSync(join(dir, name), 'https://getcoppice.app\n');
    symlinkSync(name, join(dir, 'alias'));
    git('add', '-A');

    assert.deepEqual(checkOldDomain({ source: true, root: dir }).hits, ['alias:1']);
    const result = run(['--source'], dir);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /alias:1/);
    assert.match(result.stderr, /found in 1 place/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a tracked symlink to a file fails when the target contains the old domain', () => {
  const dir = mkdtempSync(join(tmpdir(), 'coppice-old-domain-filelink-'));
  try {
    const git = initGit(dir);
    writeFileSync(join(dir, 'ok.md'), 'https://getcoppice.app\n');
    mkdirSync(join(dir, 'nested'));
    // Only the symlink is tracked, so the hit has to come from following it.
    writeFileSync(join(dir, 'nested', 'payload.txt'), `see ${OLD_DOMAIN}\n`);
    symlinkSync(join('nested', 'payload.txt'), join(dir, 'alias'));
    git('add', 'ok.md', 'alias');

    const payload = `${join('nested', 'payload.txt')}:1`;
    assert.deepEqual(checkOldDomain({ source: true, root: dir }).hits, [payload]);
    const result = run(['--source'], dir);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /payload\.txt:1/);
    assert.match(result.stderr, /found in 1 place/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a broken tracked symlink fails closed', () => {
  const dir = mkdtempSync(join(tmpdir(), 'coppice-old-domain-broken-'));
  try {
    const git = initGit(dir);
    writeFileSync(join(dir, 'ok.md'), 'https://getcoppice.app\n');
    symlinkSync('missing-target', join(dir, 'alias'));
    git('add', '-A');

    assert.throws(
      () => checkOldDomain({ source: true, root: dir }),
      /broken symlink: alias -> missing-target/,
    );
    const result = run(['--source'], dir);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /could not run: broken symlink: alias -> missing-target/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a tracked symlink that resolves outside the repository fails closed', () => {
  const parent = mkdtempSync(join(tmpdir(), 'coppice-old-domain-outside-'));
  const dir = join(parent, 'repo');
  try {
    mkdirSync(dir);
    const git = initGit(dir);
    writeFileSync(join(dir, 'ok.md'), 'https://getcoppice.app\n');
    writeFileSync(join(parent, 'outside.txt'), 'https://getcoppice.app\n');
    symlinkSync(join('..', 'outside.txt'), join(dir, 'alias'));
    git('add', '-A');

    assert.throws(
      () => checkOldDomain({ source: true, root: dir }),
      /symlink resolves outside the repository: alias -> /,
    );
    const result = run(['--source'], dir);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /could not run: symlink resolves outside the repository: alias -> /);
    assert.doesNotMatch(result.stdout, /No /);
  } finally {
    rmSync(parent, { recursive: true, force: true });
  }
});

test('the repository has no old-domain references', () => {
  const root = fileURLToPath(new URL('../..', import.meta.url));
  assert.deepEqual(checkOldDomain({ source: true, root }).hits, []);
});
