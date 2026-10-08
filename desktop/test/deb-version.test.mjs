import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdtemp, readFile, rm, writeFile, mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { describe, it } from 'node:test';
import { promisify } from 'node:util';

import { readDebVersion, stampDebVersion, stampDebsInDist } from '../scripts/deb-version.mjs';

const execFileAsync = promisify(execFile);

function dpkgAvailable() {
  return execFileAsync('dpkg-deb', ['--version']).then(
    () => true,
    () => false,
  );
}

function compare(left, op, right) {
  return execFileAsync('dpkg', ['--compare-versions', left, op, right]).then(
    () => true,
    (err) => {
      if (err && err.code === 1) return false;
      throw err;
    },
  );
}

async function writeDeb(debPath, version) {
  const root = await mkdtemp(join(tmpdir(), 'deb-fixture-'));
  try {
    await mkdir(join(root, 'DEBIAN'));
    await writeFile(
      join(root, 'DEBIAN', 'control'),
      [
        'Package: coppice-desktop',
        `Version: ${version}`,
        'Architecture: all',
        'Maintainer: Coppice Maintainers <coppice@users.noreply.github.com>',
        'Description: test package',
        '',
      ].join('\n'),
    );
    await execFileAsync('dpkg-deb', ['--root-owner-group', '--build', root, debPath]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

describe('dpkg version order', () => {
  it('orders 0.1.0~rc.7 before 0.1.0 and before a later rc', async () => {
    if (!(await dpkgAvailable())) {
      if (process.env.CI) assert.fail('dpkg is required in CI');
      return;
    }
    assert.equal(await compare('0.1.0~rc.7', 'lt', '0.1.0'), true);
    assert.equal(await compare('0.1.0', 'lt', '0.1.0~rc.7'), false);
    assert.equal(await compare('0.1.0~rc.6', 'lt', '0.1.0~rc.7'), true);
    assert.equal(await compare('0.1.0~rc.9', 'lt', '0.1.0~rc.10'), true);
    // A hyphen is a Debian revision, so the final release sorts older than every rc.
    assert.equal(await compare('0.1.0', 'lt', '0.1.0-rc.7'), true);
    assert.equal(await compare('0.1.0~rc.7', 'lt', '0.1.0-rc.6'), true);
  });
});

describe('stampDebVersion', () => {
  it('rewrites a hyphen control Version to the tilde form and keeps the file name', async () => {
    if (!(await dpkgAvailable())) {
      if (process.env.CI) assert.fail('dpkg-deb is required in CI');
      return;
    }
    const dir = await mkdtemp(join(tmpdir(), 'deb-stamp-'));
    try {
      const name = 'Coppice-0.1.0-rc.7-linux-amd64.deb';
      const deb = join(dir, name);
      await writeDeb(deb, '0.1.0-rc.7');
      const result = await stampDebVersion(deb, '0.1.0-rc.7');
      assert.equal(result.rewritten, true);
      assert.equal(result.version, '0.1.0~rc.7');
      assert.equal(basename(deb), name);
      assert.equal(await readDebVersion(deb), '0.1.0~rc.7');
      assert.equal(await compare('0.1.0~rc.7', 'lt', '0.1.0'), true);
      const control = await execFileAsync('dpkg-deb', ['-I', deb]);
      assert.match(control.stdout, /Package: coppice-desktop/);
      assert.match(control.stdout, /Version: 0\.1\.0~rc\.7/);
    } finally {
      await rm(dir, { recursive: true, force: true });
    }
  });

  it('leaves a final 0.1.0 package and an already-tilde rc package unchanged', async () => {
    if (!(await dpkgAvailable())) {
      if (process.env.CI) assert.fail('dpkg-deb is required in CI');
      return;
    }
    const dir = await mkdtemp(join(tmpdir(), 'deb-stamp-'));
    try {
      const finalDeb = join(dir, 'Coppice-0.1.0-linux-amd64.deb');
      await writeDeb(finalDeb, '0.1.0');
      const beforeFinal = await readFile(finalDeb);
      const finalResult = await stampDebVersion(finalDeb, '0.1.0');
      assert.deepEqual(finalResult, { version: '0.1.0', rewritten: false });
      assert.deepEqual(await readFile(finalDeb), beforeFinal);

      const rcDeb = join(dir, 'Coppice-0.1.0-rc.7-linux-arm64.deb');
      await writeDeb(rcDeb, '0.1.0~rc.7');
      const beforeRc = await readFile(rcDeb);
      const rcResult = await stampDebVersion(rcDeb, '0.1.0-rc.7');
      assert.deepEqual(rcResult, { version: '0.1.0~rc.7', rewritten: false });
      assert.deepEqual(await readFile(rcDeb), beforeRc);
      assert.equal(basename(rcDeb), 'Coppice-0.1.0-rc.7-linux-arm64.deb');
    } finally {
      await rm(dir, { recursive: true, force: true });
    }
  });

  it('stamps dist debs from package.json and keeps Coppice-0.1.0-rc.7-linux-amd64.deb', async () => {
    if (!(await dpkgAvailable())) {
      if (process.env.CI) assert.fail('dpkg-deb is required in CI');
      return;
    }
    const dir = await mkdtemp(join(tmpdir(), 'deb-dist-'));
    try {
      await writeFile(
        join(dir, 'package.json'),
        `${JSON.stringify({ name: 'coppice-desktop', version: '0.1.0-rc.7' })}\n`,
      );
      await mkdir(join(dir, 'dist'));
      const name = 'Coppice-0.1.0-rc.7-linux-amd64.deb';
      const deb = join(dir, 'dist', name);
      await writeDeb(deb, '0.1.0-rc.7');
      await stampDebsInDist(dir);
      assert.equal(basename(deb), name);
      assert.equal(await readDebVersion(deb), '0.1.0~rc.7');

      const tilde = join(dir, 'dist', 'Coppice-0.1.0~rc.7-linux-amd64.deb');
      await writeDeb(tilde, '0.1.0~rc.7');
      await assert.rejects(() => stampDebsInDist(dir), /file name must keep the app version/);
    } finally {
      await rm(dir, { recursive: true, force: true });
    }
  });
});
