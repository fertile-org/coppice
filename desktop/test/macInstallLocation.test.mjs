import assert from 'node:assert/strict';
import test from 'node:test';

import { macInstallBlocked } from '../src/macInstallLocation.mjs';

test('macInstallBlocked detects translocation, DMG volumes, and outside Applications', () => {
  assert.equal(
    macInstallBlocked({
      execPath: '/private/var/folders/xx/AppTranslocation/ABC/d/Coppice.app/Contents/MacOS/Coppice',
      isInApplicationsFolder: false,
    }),
    true,
  );
  assert.equal(
    macInstallBlocked({
      execPath: '/Volumes/Coppice 0.1.12/Coppice.app/Contents/MacOS/Coppice',
      isInApplicationsFolder: false,
    }),
    true,
  );
  assert.equal(
    macInstallBlocked({
      execPath: '/Users/me/Downloads/Coppice.app/Contents/MacOS/Coppice',
      isInApplicationsFolder: false,
    }),
    true,
  );
  assert.equal(
    macInstallBlocked({
      execPath: '/Applications/Coppice.app/Contents/MacOS/Coppice',
      isInApplicationsFolder: true,
    }),
    false,
  );
});
