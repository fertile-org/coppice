import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { renderLatestMacYml, zipMetadata } from '../scripts/latest-mac-yml.mjs';

test('latest-mac.yml points both architectures at the release assets', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'coppice-mac-yml-'));
  const arm = join(dir, 'Coppice-1.2.0-rc.3-mac-arm64.zip');
  const intel = join(dir, 'Coppice-1.2.0-rc.3-mac-x64.zip');
  writeFileSync(arm, Buffer.from('arm-bytes'));
  writeFileSync(intel, Buffer.from('intel-bytes'));

  const files = [
    await zipMetadata(arm, 'https://github.com/fertile-org/coppice/releases/download/v1.2.0-rc.3'),
    await zipMetadata(intel, 'https://github.com/fertile-org/coppice/releases/download/v1.2.0-rc.3'),
  ];
  const yml = renderLatestMacYml({
    version: '1.2.0-rc.3',
    files,
    releaseDate: '2026-10-17T00:00:00.000Z',
  });

  assert.match(yml, /^version: 1\.2\.0-rc\.3$/m);
  assert.match(
    yml,
    /url: 'https:\/\/github\.com\/fertile-org\/coppice\/releases\/download\/v1\.2\.0-rc\.3\/Coppice-1\.2\.0-rc\.3-mac-arm64\.zip'/,
  );
  assert.match(
    yml,
    /url: 'https:\/\/github\.com\/fertile-org\/coppice\/releases\/download\/v1\.2\.0-rc\.3\/Coppice-1\.2\.0-rc\.3-mac-x64\.zip'/,
  );
  assert.match(yml, /sha512: '/);
  assert.match(yml, /size: 9/);
  assert.match(yml, /size: 11/);
  assert.match(yml, /releaseDate: '2026-10-17T00:00:00.000Z'/);
});
