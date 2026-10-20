import assert from 'node:assert/strict';
import test from 'node:test';

import { UPDATE_INTERVAL_MS, bannerForUpdate, startUpdateLoop } from '../src/updatePlan.mjs';

const available = { version: '1.4.0-rc.2', url: 'https://example/rc' };

test('linux banner points at apt as soon as a release is newer', () => {
  assert.deepEqual(
    bannerForUpdate({ platform: 'linux', macInPlaceUpdates: false, availability: available, download: null }),
    { kind: 'linux', version: '1.4.0-rc.2' },
  );
});

test('linux offers restart once apt has installed the newer package', () => {
  assert.deepEqual(
    bannerForUpdate({
      platform: 'linux',
      macInPlaceUpdates: false,
      availability: available,
      download: null,
      linuxInstalled: '1.4.0-rc.2',
    }),
    { kind: 'ready', version: '1.4.0-rc.2' },
  );
});

test('unsigned macOS offers the release page', () => {
  assert.deepEqual(
    bannerForUpdate({ platform: 'darwin', macInPlaceUpdates: false, availability: available, download: null }),
    { kind: 'download', version: '1.4.0-rc.2', url: 'https://example/rc' },
  );
});

test('signed macOS stays quiet while the update downloads and then offers restart', () => {
  assert.equal(
    bannerForUpdate({
      platform: 'darwin',
      macInPlaceUpdates: true,
      availability: available,
      download: { phase: 'downloading', version: '1.4.0-rc.2' },
    }),
    null,
  );
  assert.deepEqual(
    bannerForUpdate({
      platform: 'darwin',
      macInPlaceUpdates: true,
      availability: available,
      download: { phase: 'downloaded', version: '1.4.0-rc.2' },
    }),
    { kind: 'ready', version: '1.4.0-rc.2' },
  );
});

test('no banner when nothing is newer', () => {
  assert.equal(
    bannerForUpdate({ platform: 'linux', macInPlaceUpdates: false, availability: null, download: null }),
    null,
  );
});

test('the update loop returns before the check finishes', async () => {
  let release;
  let intervalMs;
  const check = () =>
    new Promise((resolve) => {
      release = resolve;
    });
  const loop = startUpdateLoop({
    enabled: true,
    check,
    setIntervalImpl: (_fn, ms) => {
      intervalMs = ms;
      return 1;
    },
    clearIntervalImpl: () => {},
  });
  assert.equal(typeof release, 'function');
  assert.equal(intervalMs, UPDATE_INTERVAL_MS);
  loop.stop();
  release();
});

test('automatic updates off does not check', () => {
  let calls = 0;
  const loop = startUpdateLoop({
    enabled: false,
    check: () => {
      calls += 1;
    },
  });
  assert.equal(calls, 0);
  loop.stop();
});
