import assert from 'node:assert/strict';
import test from 'node:test';

import { latestJson } from '../scripts/latest-json.mjs';
import { readLatestManifest } from '../src/updateCheck.mjs';

test('latest.json names the tag and its release page', () => {
  const body = JSON.parse(latestJson({ tag: 'v0.1.0-rc.8', repo: 'fertile-org/coppice' }));
  assert.deepEqual(body, {
    version: '0.1.0-rc.8',
    tag: 'v0.1.0-rc.8',
    url: 'https://github.com/fertile-org/coppice/releases/tag/v0.1.0-rc.8',
  });
  assert.deepEqual(readLatestManifest(body), { tag: body.tag, url: body.url });
  assert.throws(() => latestJson({ tag: 'latest', repo: 'fertile-org/coppice' }));
});
