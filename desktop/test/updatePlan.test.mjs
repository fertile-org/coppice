import assert from 'node:assert/strict';
import test from 'node:test';

import { DOWNLOAD_PAGE_URL } from '../src/updateEndpoints.mjs';
import {
  UPDATE_INTERVAL_MS,
  bannerForUpdate,
  macInPlaceCheckResult,
  startUpdateLoop,
} from '../src/updatePlan.mjs';

const available = { version: '1.4.0-rc.2', url: 'https://example/rc' };

test('bannerForUpdate covers linux, mac download, ready, failed, move, and quiet paths', () => {
  for (const [input, expected] of [
    [
      { platform: 'linux', macInPlaceUpdates: false, availability: available, download: null },
      { kind: 'linux', version: '1.4.0-rc.2' },
    ],
    [
      {
        platform: 'linux',
        macInPlaceUpdates: false,
        availability: available,
        download: null,
        linuxInstalled: '1.4.0-rc.2',
      },
      { kind: 'ready', version: '1.4.0-rc.2' },
    ],
    [
      { platform: 'darwin', macInPlaceUpdates: false, availability: available, download: null },
      { kind: 'download', version: '1.4.0-rc.2', url: DOWNLOAD_PAGE_URL },
    ],
    [
      {
        platform: 'darwin',
        macInPlaceUpdates: true,
        availability: available,
        download: { phase: 'downloading', version: '1.4.0-rc.2' },
      },
      null,
    ],
    [
      {
        platform: 'darwin',
        macInPlaceUpdates: true,
        availability: available,
        download: { phase: 'downloaded', version: '1.4.0-rc.2' },
      },
      { kind: 'ready', version: '1.4.0-rc.2' },
    ],
    [
      {
        platform: 'darwin',
        macInPlaceUpdates: true,
        availability: available,
        download: { phase: 'failed', version: '1.4.0-rc.2', reason: 'offline' },
      },
      { kind: 'download', version: '1.4.0-rc.2', url: DOWNLOAD_PAGE_URL },
    ],
    [
      {
        platform: 'darwin',
        macInPlaceUpdates: true,
        availability: available,
        download: null,
        installBlocked: true,
      },
      { kind: 'move', version: '1.4.0-rc.2' },
    ],
    [{ platform: 'linux', macInPlaceUpdates: false, availability: null, download: null }, null],
  ]) {
    assert.deepEqual(bannerForUpdate(input), expected);
  }
});

test('macInPlaceCheckResult reports downloading, ready, and failed outcomes', () => {
  assert.deepEqual(
    macInPlaceCheckResult({
      download: { phase: 'downloading', version: '1.2.0' },
      installBlocked: false,
      latest: { status: 'available', version: '1.2.0' },
    }),
    { status: 'downloading', version: '1.2.0' },
  );
  assert.deepEqual(
    macInPlaceCheckResult({
      download: { phase: 'downloaded', version: '1.2.0' },
      installBlocked: false,
      latest: { status: 'available', version: '1.2.0' },
    }),
    { status: 'ready', version: '1.2.0' },
  );
  assert.deepEqual(
    macInPlaceCheckResult({
      download: { phase: 'failed', version: '1.2.0', reason: 'network down' },
      installBlocked: false,
      latest: { status: 'available', version: '1.2.0' },
    }),
    { status: 'failed', reason: 'network down' },
  );
  assert.deepEqual(
    macInPlaceCheckResult({
      download: null,
      installBlocked: true,
      latest: { status: 'available', version: '1.2.0' },
    }),
    {
      status: 'failed',
      reason: 'Move Coppice to the Applications folder to update in place.',
    },
  );
});

test('the update loop returns before the check finishes and respects automaticUpdates', async () => {
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

  let calls = 0;
  const off = startUpdateLoop({
    enabled: false,
    check: () => {
      calls += 1;
    },
  });
  assert.equal(calls, 0);
  off.stop();
});
