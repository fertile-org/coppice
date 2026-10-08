import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, it } from 'node:test';

import { debianPackageVersion, parseReleaseTag, setPackageVersion } from '../scripts/release-version.mjs';

describe('parseReleaseTag', () => {
  it('accepts a stable tag', () => {
    assert.deepEqual(parseReleaseTag('v1.2.3'), { version: '1.2.3', prerelease: false });
  });

  it('accepts a release candidate as a prerelease', () => {
    assert.deepEqual(parseReleaseTag('v1.2.3-rc.4'), { version: '1.2.3-rc.4', prerelease: true });
  });

  it('accepts multi-digit components', () => {
    assert.deepEqual(parseReleaseTag('v10.20.30'), { version: '10.20.30', prerelease: false });
  });

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
  ]) {
    it(`rejects ${JSON.stringify(tag)}`, () => {
      assert.throws(() => parseReleaseTag(tag), /release tag/);
    });
  }

  it('rejects non-strings', () => {
    assert.throws(() => parseReleaseTag(undefined), /release tag/);
  });
});

describe('debianPackageVersion', () => {
  it('keeps a final release as upstream X.Y.Z', () => {
    assert.equal(debianPackageVersion('0.1.0'), '0.1.0');
    assert.equal(debianPackageVersion('10.20.30'), '10.20.30');
  });

  it('uses a tilde so a release candidate sorts before the final version', () => {
    assert.equal(debianPackageVersion('0.1.0-rc.7'), '0.1.0~rc.7');
    assert.equal(debianPackageVersion('1.2.3-rc.4'), '1.2.3~rc.4');
    assert.equal(debianPackageVersion('10.20.30-rc.11'), '10.20.30~rc.11');
  });

  for (const version of ['v0.1.0-rc.7', '0.1.0-rc', '0.1.0~rc.7', '0.1.0-beta.1', '01.2.3', '']) {
    it(`rejects ${JSON.stringify(version)}`, () => {
      assert.throws(() => debianPackageVersion(version), /app version/);
    });
  }
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
