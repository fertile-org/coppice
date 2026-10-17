import test from 'node:test';
import assert from 'node:assert/strict';
import { checkForUpdate, isNewer, selectNewerRelease } from '../src/updateCheck.mjs';

test('isNewer compares semver with rc ordering', () => {
  assert.equal(isNewer('v1.2.0', '1.1.9'), true);
  assert.equal(isNewer('v1.2.0', '1.2.0'), false);
  assert.equal(isNewer('v1.2.0', '1.2.0-rc.1'), true);
  assert.equal(isNewer('v1.2.0-rc.2', '1.2.0'), false);
  assert.equal(isNewer('v1.2.0-rc.2', '1.2.0-rc.1'), true);
  assert.equal(isNewer('v1.10.0', '1.9.0'), true);
  assert.equal(isNewer('garbage', '1.0.0'), false);
  assert.equal(isNewer('v1.2.0-rc.3', '1.2.0-rc.3'), false);
});

test('selectNewerRelease follows the newest tag and includes prereleases', () => {
  const chosen = selectNewerRelease(
    [
      { tag_name: 'v1.0.0', html_url: 'https://example/stable', draft: false, prerelease: false },
      { tag_name: 'v1.2.0-rc.1', html_url: 'https://example/rc1', draft: false, prerelease: true },
      { tag_name: 'v1.2.0-rc.2', html_url: 'https://example/rc2', draft: false, prerelease: true },
      { tag_name: 'v9.0.0', html_url: 'https://example/draft', draft: true, prerelease: false },
    ],
    '1.0.0',
  );
  assert.deepEqual(chosen, { version: '1.2.0-rc.2', url: 'https://example/rc2' });
});

test('selectNewerRelease prefers a final release over an older rc', () => {
  const chosen = selectNewerRelease(
    [
      { tag_name: 'v1.2.0-rc.4', html_url: 'https://example/rc', draft: false, prerelease: true },
      { tag_name: 'v1.2.0', html_url: 'https://example/final', draft: false, prerelease: false },
    ],
    '1.1.0',
  );
  assert.deepEqual(chosen, { version: '1.2.0', url: 'https://example/final' });
});

test('checkForUpdate reads the release list so prereleases are visible', async () => {
  let requested;
  let options;
  const fetchImpl = async (url, opts) => {
    requested = url;
    options = opts;
    return {
      ok: true,
      json: async () => [
        { tag_name: 'v1.0.0', html_url: 'https://example/stable', draft: false },
        { tag_name: 'v1.4.0-rc.2', html_url: 'https://example/rc', draft: false, prerelease: true },
      ],
    };
  };
  const result = await checkForUpdate({
    repo: 'fertile-org/coppice',
    currentVersion: '1.0.0',
    fetchImpl,
  });
  assert.deepEqual(result, {
    status: 'available',
    version: '1.4.0-rc.2',
    url: 'https://example/rc',
  });
  assert.equal(requested, 'https://api.github.com/repos/fertile-org/coppice/releases?per_page=10');
  assert.ok(options.signal instanceof AbortSignal, 'fetch is bounded by an abort signal');
});

test('checkForUpdate reports up to date when nothing newer is published', async () => {
  const fetchImpl = async () => ({
    ok: true,
    json: async () => [{ tag_name: 'v1.0.0', html_url: 'https://example/same', draft: false }],
  });
  assert.deepEqual(
    await checkForUpdate({ repo: 'r/r', currentVersion: '1.0.0', fetchImpl }),
    { status: 'up-to-date' },
  );
});

test('checkForUpdate fails quietly on a non-200 or a network error', async () => {
  const notFound = async () => ({ ok: false, status: 404, json: async () => ({}) });
  assert.deepEqual(
    await checkForUpdate({ repo: 'r/r', currentVersion: '1.0.0', fetchImpl: notFound }),
    { status: 'error' },
  );
  const rejecting = async () => {
    throw new Error('offline');
  };
  assert.deepEqual(
    await checkForUpdate({ repo: 'r/r', currentVersion: '1.0.0', fetchImpl: rejecting }),
    { status: 'error' },
  );
});

test('checkForUpdate reads latest.json from the update host first', async () => {
  const requested = [];
  const fetchImpl = async (url) => {
    requested.push(url);
    return {
      ok: true,
      json: async () => ({
        version: '1.4.0',
        tag: 'v1.4.0',
        url: 'https://github.com/fertile-org/coppice/releases/tag/v1.4.0',
      }),
    };
  };
  const result = await checkForUpdate({
    repo: 'fertile-org/coppice',
    currentVersion: '1.3.0',
    updateBase: 'https://apt.getcoppice.app',
    fetchImpl,
  });
  assert.deepEqual(result, {
    status: 'available',
    version: '1.4.0',
    url: 'https://github.com/fertile-org/coppice/releases/tag/v1.4.0',
  });
  assert.deepEqual(requested, ['https://apt.getcoppice.app/updates/latest.json']);
  assert.deepEqual(
    await checkForUpdate({ repo: 'r/r', currentVersion: '1.4.0', updateBase: 'https://apt.getcoppice.app', fetchImpl }),
    { status: 'up-to-date' },
  );
});

test('checkForUpdate falls back to the release list when latest.json is missing or odd', async () => {
  for (const manifest of [
    { ok: false, status: 404, json: async () => ({}) },
    { ok: true, json: async () => ({ tag: 'nonsense', url: 'https://x' }) },
    { ok: true, json: async () => ({ tag: 'v9.0.0', url: 'http://plain' }) },
  ]) {
    const requested = [];
    const fetchImpl = async (url) => {
      requested.push(url);
      if (url.endsWith('/latest.json')) return manifest;
      return {
        ok: true,
        json: async () => [{ tag_name: 'v1.5.0', html_url: 'https://example/1.5', draft: false }],
      };
    };
    const result = await checkForUpdate({
      repo: 'fertile-org/coppice',
      currentVersion: '1.0.0',
      updateBase: 'https://apt.getcoppice.app',
      fetchImpl,
    });
    assert.deepEqual(result, { status: 'available', version: '1.5.0', url: 'https://example/1.5' });
    assert.equal(requested.length, 2);
  }
});
