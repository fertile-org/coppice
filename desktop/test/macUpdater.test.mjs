import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import test from 'node:test';

import { MAC_UPDATE_FEED } from '../src/updateEndpoints.mjs';
import {
  attachMacUpdater,
  configureMacFeed,
  IN_PLACE_NOT_AVAILABLE_REASON,
} from '../src/macUpdater.mjs';
import { bannerForUpdate } from '../src/updatePlan.mjs';
import { DOWNLOAD_PAGE_URL } from '../src/updateEndpoints.mjs';
import { readUpdateBase, readUpdaterMode } from '../src/readUpdaterMode.mjs';

test('in-place updates use the Pages feed and still accept prereleases', () => {
  const updater = {
    feed: null,
    setFeedURL(feed) {
      this.feed = feed;
    },
    allowPrerelease: false,
    allowDowngrade: true,
    autoDownload: false,
    autoInstallOnAppQuit: false,
  };
  configureMacFeed(updater);
  assert.deepEqual(updater.feed, { provider: 'generic', url: MAC_UPDATE_FEED });
  assert.equal(MAC_UPDATE_FEED.endsWith('/'), true);
  assert.equal(updater.allowPrerelease, true);
  assert.equal(updater.allowDowngrade, false);
  assert.equal(updater.autoDownload, true);
  assert.equal(updater.autoInstallOnAppQuit, true);
});

test('a downloaded update can restart the app', () => {
  const updater = new EventEmitter();
  let installed = null;
  updater.checkForUpdates = () => 'checking';
  updater.quitAndInstall = (silent, force) => {
    installed = [silent, force];
  };
  const seen = [];
  const errors = [];
  const logs = [];
  const controller = attachMacUpdater(updater, {
    onDownloaded: (info) => seen.push(info),
    onError: (error) => errors.push(error.message),
    onLog: (line) => logs.push(line),
  });
  updater.emit('checking-for-update');
  updater.emit('update-downloaded', { version: '1.2.0-rc.3' });
  updater.emit('error', new Error('offline'));
  assert.deepEqual(seen, [{ version: '1.2.0-rc.3' }]);
  assert.deepEqual(errors, ['offline']);
  assert.ok(logs.includes('checking-for-update'));
  assert.ok(logs.some((line) => line.startsWith('update-downloaded: 1.2.0-rc.3')));
  assert.ok(logs.some((line) => line.startsWith('error: offline')));
  assert.equal(controller.check(), 'checking');
  controller.install();
  assert.deepEqual(installed, [false, true]);
  controller.setAutoInstallOnQuit(false);
  assert.equal(updater.autoInstallOnAppQuit, false);
});

test('update-not-available while downloading fails in place and shows Download', () => {
  const updater = new EventEmitter();
  updater.checkForUpdates = () => {};
  updater.quitAndInstall = () => {};
  let notAvailable = 0;
  let download = { phase: 'downloading', version: '1.4.0-rc.2' };
  const logs = [];
  attachMacUpdater(updater, {
    onNotAvailable() {
      notAvailable += 1;
      if (download?.phase === 'downloading') {
        download = {
          phase: 'failed',
          version: download.version,
          reason: IN_PLACE_NOT_AVAILABLE_REASON,
        };
      }
    },
    onLog: (line) => logs.push(line),
  });
  updater.emit('update-not-available', { version: '1.4.0-rc.1' });
  updater.emit('download-progress', { percent: 12.3 });
  assert.equal(notAvailable, 1);
  assert.ok(logs.includes('update-not-available: 1.4.0-rc.1'));
  assert.equal(
    logs.some((line) => line.startsWith('download-progress')),
    false,
    'download-progress must not flood updater.log',
  );
  assert.deepEqual(
    bannerForUpdate({
      platform: 'darwin',
      macInPlaceUpdates: true,
      availability: { version: '1.4.0-rc.2', url: 'https://example/rc' },
      download,
    }),
    { kind: 'download', version: '1.4.0-rc.2', url: DOWNLOAD_PAGE_URL },
  );
  assert.match(download.reason, /latest-mac\.yml/);
});

test('updater mode and packaged update base come from updater-mode.json', () => {
  for (const [raw, expected] of [
    ['{"macInPlaceUpdates":true}\n', true],
    ['{"macInPlaceUpdates":false}\n', false],
    ['', false],
    ['{', false],
  ]) {
    assert.equal(readUpdaterMode(raw), expected, raw);
  }
  assert.equal(readUpdateBase('{"updateBase":"https://apt.example.com"}'), 'https://apt.example.com');
  assert.equal(readUpdateBase('{"updateBase":"  "}'), null);
  assert.equal(readUpdateBase(''), null);
  assert.equal(readUpdateBase('{'), null);
});
