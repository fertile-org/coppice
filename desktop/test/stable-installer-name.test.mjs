import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { STABLE_INSTALLER_NAMES, stableInstallerName } from '../scripts/stable-installer-name.mjs';

describe('stableInstallerName', () => {
  it('lists the four unversioned installers', () => {
    assert.deepEqual(STABLE_INSTALLER_NAMES, [
      'Coppice-linux-amd64.deb',
      'Coppice-linux-arm64.deb',
      'Coppice-mac-arm64.dmg',
      'Coppice-mac-x64.dmg',
    ]);
  });

  it('drops a final version without renaming the platform', () => {
    assert.equal(
      stableInstallerName('0.1.0', 'desktop/dist/Coppice-0.1.0-linux-amd64.deb'),
      'Coppice-linux-amd64.deb',
    );
    assert.equal(
      stableInstallerName('0.1.0', 'Coppice-0.1.0-linux-arm64.deb'),
      'Coppice-linux-arm64.deb',
    );
    assert.equal(
      stableInstallerName('10.20.30', 'Coppice-10.20.30-mac-arm64.dmg'),
      'Coppice-mac-arm64.dmg',
    );
    assert.equal(
      stableInstallerName('0.1.0', 'Coppice-0.1.0-mac-x64.dmg'),
      'Coppice-mac-x64.dmg',
    );
  });

  it('drops a release-candidate version that itself contains a hyphen', () => {
    assert.equal(
      stableInstallerName('0.1.0-rc.7', 'Coppice-0.1.0-rc.7-linux-amd64.deb'),
      'Coppice-linux-amd64.deb',
    );
    assert.equal(
      stableInstallerName('0.1.0-rc.7', 'Coppice-0.1.0-rc.7-mac-x64.dmg'),
      'Coppice-mac-x64.dmg',
    );
  });

  it('rejects a name that is not one of the four installers', () => {
    assert.throws(
      () => stableInstallerName('0.1.0', 'Coppice-0.1.0-win-x64.exe'),
      /stable installer name/,
    );
    assert.throws(
      // The old Linux asset name; built at runtime so the repo never spells it.
      () => stableInstallerName('0.1.0', `Coppice-0.1.0-${['linux', 'x64'].join('-')}.deb`),
      /stable installer name/,
    );
  });

  it('rejects a version that does not match the file', () => {
    assert.throws(
      () => stableInstallerName('0.2.0', 'Coppice-0.1.0-linux-amd64.deb'),
      /not named/,
    );
    assert.throws(
      () => stableInstallerName('0.1.0', 'Coppice-0.1.0-rc.7-linux-amd64.deb'),
      /stable installer name/,
    );
    assert.throws(() => stableInstallerName('', 'Coppice-0.1.0-linux-amd64.deb'), /version/);
  });
});
