import assert from 'node:assert/strict';
import test from 'node:test';

import { selectPublishedRelease } from '../scripts/select-published-release.mjs';

test('manual dispatch follows the newest published release, including an rc', () => {
  const chosen = selectPublishedRelease([
    { tagName: 'v1.0.0', isDraft: false },
    { tagName: 'v1.3.0-rc.1', isDraft: false },
    { tagName: 'v9.0.0', isDraft: true },
    { tagName: 'not-a-release', isDraft: false },
  ]);
  assert.deepEqual(chosen, { tag: 'v1.3.0-rc.1', version: '1.3.0-rc.1' });
});

test('a final release beats an older rc', () => {
  const chosen = selectPublishedRelease([
    { tag_name: 'v1.3.0-rc.4', draft: false },
    { tag_name: 'v1.3.0', draft: false },
  ]);
  assert.deepEqual(chosen, { tag: 'v1.3.0', version: '1.3.0' });
});
