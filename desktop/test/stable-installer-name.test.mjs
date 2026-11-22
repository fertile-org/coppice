import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { STABLE_INSTALLER_NAMES, stableInstallerName } from '../scripts/stable-installer-name.mjs';

describe('stableInstallerName', () => {
  it('lists the four unversioned installers and drops versioned names', () => {
    assert.deepEqual(STABLE_INSTALLER_NAMES, [
      'Coppice-linux-amd64.deb',
      'Coppice-linux-arm64.deb',
      'Coppice-mac-arm64.dmg',
      'Coppice-mac-x64.dmg',
    ]);
    for (const [version, file, expected] of [
      ['0.1.0', 'desktop/dist/Coppice-0.1.0-linux-amd64.deb', 'Coppice-linux-amd64.deb'],
      ['0.1.0', 'Coppice-0.1.0-linux-arm64.deb', 'Coppice-linux-arm64.deb'],
      ['10.20.30', 'Coppice-10.20.30-mac-arm64.dmg', 'Coppice-mac-arm64.dmg'],
      ['0.1.0', 'Coppice-0.1.0-mac-x64.dmg', 'Coppice-mac-x64.dmg'],
      ['0.1.0-rc.7', 'Coppice-0.1.0-rc.7-linux-amd64.deb', 'Coppice-linux-amd64.deb'],
      ['0.1.0-rc.7', 'Coppice-0.1.0-rc.7-mac-x64.dmg', 'Coppice-mac-x64.dmg'],
    ]) {
      assert.equal(stableInstallerName(version, file), expected, file);
    }
  });

  it('rejects names and versions that are not one of the four installers', () => {
    for (const [version, file, pattern] of [
      ['0.1.0', 'Coppice-0.1.0-win-x64.exe', /stable installer name/],
      // The old Linux asset name; built at runtime so the repo never spells it.
      ['0.1.0', `Coppice-0.1.0-${['linux', 'x64'].join('-')}.deb`, /stable installer name/],
      ['0.2.0', 'Coppice-0.1.0-linux-amd64.deb', /not named/],
      ['0.1.0', 'Coppice-0.1.0-rc.7-linux-amd64.deb', /stable installer name/],
      ['', 'Coppice-0.1.0-linux-amd64.deb', /version/],
    ]) {
      assert.throws(() => stableInstallerName(version, file), pattern, file);
    }
  });
});
