import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, it } from 'node:test';

import { debianPackageVersion, parseReleaseTag, setPackageVersion } from '../scripts/release-version.mjs';

describe('parseReleaseTag', () => {
  it('parses stable and prerelease tags', () => {
    for (const [tag, expected] of [
      ['v1.2.3', { version: '1.2.3', prerelease: false }],
      ['v1.2.3-rc.4', { version: '1.2.3-rc.4', prerelease: true }],
      ['v10.20.30', { version: '10.20.30', prerelease: false }],
    ]) {
      assert.deepEqual(parseReleaseTag(tag), expected, tag);
    }
  });

  it('rejects invalid tags', () => {
    for (const tag of [
      '1.2.3',
      'v1.2',
      'v1.2.3-beta',
      'v1.2.3-rc',
      'v1.2.3-rc.',
      'v01.2.3',
      'v1.2.3 ',
      'refs/tags/v1.2.3',
      '',
      undefined,
    ]) {
      assert.throws(() => parseReleaseTag(tag), /release tag/, String(tag));
    }
  });
});

describe('debianPackageVersion', () => {
  it('keeps finals and tilde-maps release candidates', () => {
    for (const [version, expected] of [
      ['0.1.0', '0.1.0'],
      ['10.20.30', '10.20.30'],
      ['0.1.0-rc.7', '0.1.0~rc.7'],
      ['1.2.3-rc.4', '1.2.3~rc.4'],
      ['10.20.30-rc.11', '10.20.30~rc.11'],
    ]) {
      assert.equal(debianPackageVersion(version), expected, version);
    }
  });

  it('rejects invalid app versions', () => {
    for (const version of ['v0.1.0-rc.7', '0.1.0-rc', '0.1.0~rc.7', '0.1.0-beta.1', '01.2.3', '']) {
      assert.throws(() => debianPackageVersion(version), /app version/, version);
    }
  });
});

describe('setPackageVersion', () => {
  it('rewrites only the version and keeps the trailing newline', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'release-version-'));
    try {
      const file = join(dir, 'package.json');
      await writeFile(file, `${JSON.stringify({ name: 'x', version: '0.1.0', private: true }, null, 2)}\n`);
      await setPackageVersion(file, '1.2.3-rc.4');
      const text = await readFile(file, 'utf8');
      assert.ok(text.endsWith('}\n'));
      assert.deepEqual(JSON.parse(text), { name: 'x', version: '1.2.3-rc.4', private: true });
    } finally {
      await rm(dir, { recursive: true, force: true });
    }
  });
});
