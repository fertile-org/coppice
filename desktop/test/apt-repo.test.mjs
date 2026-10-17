import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  chmodSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import {
  APT_SOURCE_LINE,
  buildAptRepo,
  installAptPublicKey,
  poolFilename,
  renderPackageEntry,
} from '../scripts/apt-repo.mjs';
import { APT_KEYRING_PATH, aptSourceLine, macUpdateFeed } from '../src/updateEndpoints.mjs';

test('package entries keep the Debian version and a pool path', () => {
  assert.equal(
    poolFilename('coppice-desktop', '0.1.0~rc.7', 'arm64'),
    'pool/main/c/coppice-desktop/coppice-desktop_0.1.0~rc.7_arm64.deb',
  );
  const entry = renderPackageEntry(
    {
      Package: 'coppice-desktop',
      Version: '0.1.0~rc.7',
      Architecture: 'amd64',
      Maintainer: 'Coppice Maintainers <coppice@users.noreply.github.com>',
      Description: 'Local board\nSecond line',
    },
    {
      filename: 'pool/main/c/coppice-desktop/coppice-desktop_0.1.0~rc.7_amd64.deb',
      size: 12,
      md5: 'm',
      sha1: 's',
      sha256: 'h',
    },
  );
  assert.match(entry, /^Version: 0\.1\.0~rc\.7$/m);
  assert.match(entry, /^Architecture: amd64$/m);
  assert.match(entry, /Description: Local board\n Second line\n/);
});

test('an empty public key does not leave a keyring behind', () => {
  const dir = mkdtempSync(join(tmpdir(), 'coppice-apt-key-'));
  const out = join(dir, 'coppice-archive-keyring.gpg');
  writeFileSync(out, 'stale');
  assert.equal(installAptPublicKey({ publicKeyArmored: '  ', outPath: out }), false);
  assert.equal(readOrNull(out), null);
});

test('packaging installs a throwaway public key and refuses a private key', () => {
  const dir = mkdtempSync(join(tmpdir(), 'coppice-apt-pub-'));
  const gpgHome = join(dir, 'gnupg');
  mkdirSync(gpgHome, { mode: 0o700 });
  const gen = spawnSync('gpg', ['--batch', '--homedir', gpgHome, '--gen-key'], {
    input: [
      '%no-protection',
      'Key-Type: RSA',
      'Key-Length: 2048',
      'Name-Real: Coppice Apt Public Test',
      'Expire-Date: 0',
      '%commit',
      '',
    ].join('\n'),
    encoding: 'utf8',
  });
  assert.equal(gen.status, 0, gen.stderr);
  const pub = spawnSync('gpg', ['--homedir', gpgHome, '--batch', '--armor', '--export'], { encoding: 'utf8' });
  assert.equal(pub.status, 0, pub.stderr);
  const secret = spawnSync('gpg', ['--homedir', gpgHome, '--batch', '--armor', '--export-secret-keys'], {
    encoding: 'utf8',
  });
  assert.equal(secret.status, 0, secret.stderr);
  const out = join(dir, 'nested', 'coppice-archive-keyring.gpg');
  assert.equal(installAptPublicKey({ publicKeyArmored: pub.stdout, outPath: out }), true);
  const shown = spawnSync('gpg', ['--batch', '--show-keys', out], { encoding: 'utf8' });
  assert.equal(shown.status, 0, shown.stderr);
  assert.match(shown.stdout, /Coppice Apt Public Test/);
  assert.throws(() => installAptPublicKey({ publicKeyArmored: secret.stdout, outPath: out }), /public key/);
  chmodSync(gpgHome, 0o700);
  rmSync(dir, { recursive: true, force: true });
});

test('a throwaway key signs amd64 and arm64 metadata', () => {
  const dir = mkdtempSync(join(tmpdir(), 'coppice-apt-repo-'));
  const gpgHome = join(dir, 'gnupg');
  mkdirSync(gpgHome, { mode: 0o700 });
  const gen = spawnSync(
    'gpg',
    ['--batch', '--homedir', gpgHome, '--gen-key'],
    {
      input: [
        '%no-protection',
        'Key-Type: RSA',
        'Key-Length: 2048',
        'Name-Real: Coppice Apt Test',
        'Name-Email: apt-test@example.com',
        'Expire-Date: 0',
        '%commit',
        '',
      ].join('\n'),
      encoding: 'utf8',
    },
  );
  assert.equal(gen.status, 0, gen.stderr);

  const exported = spawnSync(
    'gpg',
    ['--homedir', gpgHome, '--batch', '--armor', '--export-secret-keys'],
    { encoding: 'utf8' },
  );
  assert.equal(exported.status, 0, exported.stderr);

  const debs = ['amd64', 'arm64'].map((arch) => makeDeb(dir, arch));
  const out = join(dir, 'site');
  const built = buildAptRepo({
    debPaths: debs,
    outDir: out,
    keyArmored: exported.stdout,
    date: new Date('2026-10-17T00:00:00Z'),
  });
  assert.deepEqual(built.architectures, ['amd64', 'arm64']);

  const release = readFileSync(join(out, 'dists/stable/Release'), 'utf8');
  assert.match(release, /^Suite: stable$/m);
  for (const arch of ['amd64', 'arm64']) {
    const packagesPath = join(out, `dists/stable/main/binary-${arch}/Packages`);
    const sha = createHash('sha256').update(readFileSync(packagesPath)).digest('hex');
    assert.match(release, new RegExp(` ${sha} \\d+ main/binary-${arch}/Packages$`, 'm'));
  }
  assert.match(release, /^Architectures: amd64 arm64$/m);
  assert.match(release, /^Components: main$/m);
  for (const arch of ['amd64', 'arm64']) {
    const packages = readFileSync(join(out, `dists/stable/main/binary-${arch}/Packages`), 'utf8');
    assert.match(packages, /^Package: coppice-desktop$/m);
    assert.match(packages, new RegExp(`^Architecture: ${arch}$`, 'm'));
    assert.match(packages, /^Filename: pool\/main\/c\/coppice-desktop\/coppice-desktop_0\.0\.1_/m);
  }

  const verify = spawnSync(
    'gpg',
    ['--homedir', gpgHome, '--batch', '--verify', 'dists/stable/Release.gpg', 'dists/stable/Release'],
    { cwd: out, encoding: 'utf8' },
  );
  assert.equal(verify.status, 0, verify.stderr);
  const inRelease = spawnSync(
    'gpg',
    ['--homedir', gpgHome, '--batch', '--verify', 'dists/stable/InRelease'],
    { cwd: out, encoding: 'utf8' },
  );
  assert.equal(inRelease.status, 0, inRelease.stderr);
  assert.equal(readFileSync(join(out, '.nojekyll'), 'utf8'), '');

  chmodSync(gpgHome, 0o700);
  rmSync(dir, { recursive: true, force: true });
});

test('the installed apt source uses signed-by and the base URL lives in one place', () => {
  const script = readFileSync(new URL('../build/linux/after-install.sh', import.meta.url), 'utf8');
  assert.match(script, /resources\/apt\/coppice\.list/);
  assert.equal(script.includes(APT_KEYRING_PATH), true);
  assert.equal(script.includes('github.io'), false);
  assert.equal(APT_SOURCE_LINE, aptSourceLine());
  assert.equal(
    aptSourceLine('https://apt.example.com/'),
    `deb [arch=amd64,arm64 signed-by=${APT_KEYRING_PATH}] https://apt.example.com stable main`,
  );
  assert.equal(macUpdateFeed('https://apt.example.com/'), 'https://apt.example.com/updates/');
  const remove = readFileSync(new URL('../build/linux/after-remove.sh', import.meta.url), 'utf8');
  assert.equal(remove.includes(APT_KEYRING_PATH), true);
  assert.match(remove, /sources\.list\.d\/coppice\.list/);
});

function readOrNull(file) {
  try {
    return readFileSync(file);
  } catch {
    return null;
  }
}

function makeDeb(dir, arch) {
  const root = join(dir, `pkg-${arch}`);
  mkdirSync(join(root, 'DEBIAN'), { recursive: true });
  writeFileSync(
    join(root, 'DEBIAN/control'),
    [
      'Package: coppice-desktop',
      'Version: 0.0.1',
      `Architecture: ${arch}`,
      'Maintainer: Coppice Apt Test <apt-test@example.com>',
      'Description: Coppice test package',
      '',
    ].join('\n'),
  );
  const deb = join(dir, `coppice_${arch}.deb`);
  const built = spawnSync('dpkg-deb', ['--root-owner-group', '--build', root, deb], { encoding: 'utf8' });
  assert.equal(built.status, 0, built.stderr);
  return deb;
}
