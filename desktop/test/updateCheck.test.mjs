import test from 'node:test';
import assert from 'node:assert/strict';
import { checkForUpdate, isNewer } from '../src/updateCheck.mjs';

test('isNewer compares semver with rc ordering', () => {
  assert.equal(isNewer('v1.2.0', '1.1.9'), true);
  assert.equal(isNewer('v1.2.0', '1.2.0'), false);
  assert.equal(isNewer('v1.2.0', '1.2.0-rc.1'), true);
  assert.equal(isNewer('v1.2.0-rc.2', '1.2.0'), false);
  assert.equal(isNewer('v1.2.0-rc.2', '1.2.0-rc.1'), true);
  assert.equal(isNewer('v1.10.0', '1.9.0'), true);
  assert.equal(isNewer('garbage', '1.0.0'), false);
});

test('checkForUpdate returns the newer release', async () => {
  let requested;
  const fetchImpl = async (url) => {
    requested = url;
    return { ok: true, json: () => ({ tag_name: 'v9.0.0', html_url: 'u' }) };
  };
  const result = await checkForUpdate({ repo: 'fertile-org/coppice', currentVersion: '1.0.0', fetchImpl });
  assert.deepEqual(result, { version: '9.0.0', url: 'u' });
  assert.equal(requested, 'https://api.github.com/repos/fertile-org/coppice/releases/latest');
});

test('checkForUpdate returns null when not newer, on non-200, or on error', async () => {
  const same = async () => ({ ok: true, json: () => ({ tag_name: 'v1.0.0', html_url: 'u' }) });
  assert.equal(await checkForUpdate({ repo: 'r/r', currentVersion: '1.0.0', fetchImpl: same }), null);
  const notFound = async () => ({ ok: false, status: 404, json: () => ({}) });
  assert.equal(await checkForUpdate({ repo: 'r/r', currentVersion: '1.0.0', fetchImpl: notFound }), null);
  const rejecting = async () => {
    throw new Error('offline');
  };
  assert.equal(await checkForUpdate({ repo: 'r/r', currentVersion: '1.0.0', fetchImpl: rejecting }), null);
});
