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
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import {
  APT_SOURCE_LINE,
  assertKeyringHasFingerprint,
  assertSigningKeyMatchesFingerprint,
  buildAptRepo,
  fingerprintsFromColonListing,
  normalizeFingerprint,
  poolFilename,
  publicKeyringFingerprints,
  renderPackageEntry,
} from '../scripts/apt-repo.mjs';
import { APT_KEYRING_FILE, stageAptResources } from '../scripts/dist.mjs';
import { APT_KEYRING_PATH, MAC_UPDATE_FEED, aptSourceLine, macUpdateFeed, pagesCname } from '../src/updateEndpoints.mjs';

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

test('packaging copies the committed public keyring', () => {
  const keyring = fileURLToPath(new URL(`../${APT_KEYRING_FILE}`, import.meta.url));
  const shown = spawnSync('gpg', ['--batch', '--show-keys', keyring], { encoding: 'utf8' });
  assert.equal(shown.status, 0, shown.stderr);
  assert.match(shown.stdout, /Coppice Apt/);
  const packets = spawnSync('gpg', ['--batch', '--list-packets', keyring], { encoding: 'utf8' });
  assert.equal(packets.status, 0, packets.stderr);
  assert.match(packets.stdout, /public key packet/);
  assert.doesNotMatch(`${packets.stdout}\n${packets.stderr}`, /secret key/i);

  const dir = mkdtempSync(join(tmpdir(), 'coppice-apt-stage-'));
  stageAptResources({ COPPICE_UPDATE_BASE: 'https://apt.example.com/' }, dir, keyring);
  assert.deepEqual(readFileSync(join(dir, 'apt/coppice-archive-keyring.gpg')), readFileSync(keyring));
  assert.equal(
    readFileSync(join(dir, 'apt/coppice.list'), 'utf8'),
    `${aptSourceLine('https://apt.example.com/')}\n`,
  );
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
  assert.equal(
    APT_SOURCE_LINE,
    `deb [arch=amd64,arm64 signed-by=${APT_KEYRING_PATH}] https://apt.getcoppice.dev stable main`,
  );
  assert.equal(MAC_UPDATE_FEED, 'https://apt.getcoppice.dev/updates/');
  assert.equal(pagesCname(), 'apt.getcoppice.dev');
  assert.equal(pagesCname('https://apt.example.com/'), 'apt.example.com');
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

test('colon fingerprint fields are the tenth column', () => {
  assert.equal(normalizeFingerprint('ae0e c971'), 'AE0EC971');
  assert.deepEqual(
    fingerprintsFromColonListing('fpr:::::::::PRIMARY:\nssb:x\nfpr:::::::::SUBKEY:\n'),
    ['PRIMARY', 'SUBKEY'],
  );
});

test('the committed keyring exposes a fingerprint without pinning the test key', () => {
  const keyring = fileURLToPath(new URL(`../${APT_KEYRING_FILE}`, import.meta.url));
  const [first] = publicKeyringFingerprints(keyring);
  assert.match(first, /^[0-9A-F]{40}$/);
});

test('keyring and signing-key checks follow COPPICE_APT_KEY_FINGERPRINT', () => {
  const dir = mkdtempSync(join(tmpdir(), 'coppice-apt-fpr-'));
  const gpgHome = join(dir, 'gnupg');
  mkdirSync(gpgHome, { mode: 0o700 });
  const gen = spawnSync('gpg', ['--batch', '--homedir', gpgHome, '--gen-key'], {
    input: [
      '%no-protection',
      'Key-Type: RSA',
      'Key-Length: 2048',
      'Name-Real: Coppice Apt Fingerprint',
      'Name-Email: apt-fpr@example.com',
      'Expire-Date: 0',
      '%commit',
      '',
    ].join('\n'),
    encoding: 'utf8',
  });
  assert.equal(gen.status, 0, gen.stderr);

  const listed = spawnSync(
    'gpg',
    ['--homedir', gpgHome, '--batch', '--with-colons', '--list-secret-keys'],
    { encoding: 'utf8' },
  );
  assert.equal(listed.status, 0, listed.stderr);
  const fingerprint = fingerprintsFromColonListing(listed.stdout)[0];
  assert.match(fingerprint, /^[0-9A-F]{40}$/);
  const spaced = fingerprint.toLowerCase().replace(/(.{4})/g, '$1 ').trim();

  const pub = join(dir, 'keyring.gpg');
  const exportedPub = spawnSync(
    'gpg',
    ['--homedir', gpgHome, '--batch', '--export', '--output', pub],
    { encoding: 'utf8' },
  );
  assert.equal(exportedPub.status, 0, exportedPub.stderr);
  const secret = spawnSync(
    'gpg',
    ['--homedir', gpgHome, '--batch', '--armor', '--export-secret-keys'],
    { encoding: 'utf8' },
  );
  assert.equal(secret.status, 0, secret.stderr);
  const secretPath = join(dir, 'key.asc');
  writeFileSync(secretPath, secret.stdout, { mode: 0o600 });
  const passPath = join(dir, 'pass');
  writeFileSync(passPath, '', { mode: 0o600 });

  assertKeyringHasFingerprint(pub, spaced);
  assertSigningKeyMatchesFingerprint(secret.stdout, '', spaced);
  assert.throws(() => assertKeyringHasFingerprint(pub, ''), /COPPICE_APT_KEY_FINGERPRINT is empty/);
  assert.throws(() => assertKeyringHasFingerprint(pub, fingerprint.slice(0, 16)), /40-character/);
  assert.throws(
    () => assertKeyringHasFingerprint(pub, 'A'.repeat(40)),
    /does not contain COPPICE_APT_KEY_FINGERPRINT/,
  );
  assert.throws(
    () => assertSigningKeyMatchesFingerprint(secret.stdout, '', 'B'.repeat(40)),
    /does not match COPPICE_APT_KEY_FINGERPRINT/,
  );
  assert.throws(() => assertSigningKeyMatchesFingerprint('not a key', '', fingerprint), /fingerprint|gpg/i);

  const script = fileURLToPath(new URL('../scripts/apt-repo.mjs', import.meta.url));
  const match = spawnSync(
    process.execPath,
    [script, 'check-keyring', '--keyring', pub, '--fingerprint', spaced],
    { encoding: 'utf8' },
  );
  assert.equal(match.status, 0, match.stderr);
  const mismatch = spawnSync(
    process.execPath,
    [
      script,
      'check-signing-key',
      '--key',
      secretPath,
      '--passphrase-file',
      passPath,
      '--fingerprint',
      'C'.repeat(40),
    ],
    { encoding: 'utf8' },
  );
  assert.notEqual(mismatch.status, 0);
  assert.match(mismatch.stderr, /does not match COPPICE_APT_KEY_FINGERPRINT/);

  chmodSync(gpgHome, 0o700);
  rmSync(dir, { recursive: true, force: true });
});

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
