import assert from 'node:assert/strict';
import test from 'node:test';

import { selectForEvent, selectPublishedRelease } from '../scripts/select-published-release.mjs';

test('selectPublishedRelease follows the newest published release', () => {
  for (const [releases, expected] of [
    [
      [
        { tagName: 'v1.0.0', isDraft: false },
        { tagName: 'v1.3.0-rc.1', isDraft: false },
        { tagName: 'v9.0.0', isDraft: true },
        { tagName: 'not-a-release', isDraft: false },
      ],
      { tag: 'v1.3.0-rc.1', version: '1.3.0-rc.1' },
    ],
    [
      [
        { tag_name: 'v1.3.0-rc.4', draft: false },
        { tag_name: 'v1.3.0', draft: false },
      ],
      { tag: 'v1.3.0', version: '1.3.0' },
    ],
  ]) {
    assert.deepEqual(selectPublishedRelease(releases), expected);
  }

  // A published event for an older release still mirrors the newest one.
  assert.deepEqual(
    selectForEvent('v1.0.0-rc.2', [
      { tagName: 'v1.0.0-rc.2', isDraft: false },
      { tagName: 'v1.0.0', isDraft: false },
    ]),
    { tag: 'v1.0.0', version: '1.0.0' },
  );
});
