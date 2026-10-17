import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import test from 'node:test';

import { MAC_UPDATE_FEED } from '../src/updateEndpoints.mjs';
import { attachMacUpdater, configureMacFeed } from '../src/macUpdater.mjs';
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
  const controller = attachMacUpdater(updater, {
    onDownloaded: (info) => seen.push(info),
    onError: (error) => errors.push(error.message),
  });
  updater.emit('update-downloaded', { version: '1.2.0-rc.3' });
  updater.emit('error', new Error('offline'));
  assert.deepEqual(seen, [{ version: '1.2.0-rc.3' }]);
  assert.deepEqual(errors, ['offline']);
  assert.equal(controller.check(), 'checking');
  controller.install();
  assert.deepEqual(installed, [false, true]);
  controller.setAutoInstallOnQuit(false);
  assert.equal(updater.autoInstallOnAppQuit, false);
});

test('updater mode is on only when the package was signed', () => {
  assert.equal(readUpdaterMode('{"macInPlaceUpdates":true}\n'), true);
  assert.equal(readUpdaterMode('{"macInPlaceUpdates":false}\n'), false);
  assert.equal(readUpdaterMode(''), false);
  assert.equal(readUpdaterMode('{'), false);
});

test('a packaged update base overrides the default feed host', () => {
  assert.equal(readUpdateBase('{"updateBase":"https://apt.example.com"}'), 'https://apt.example.com');
  assert.equal(readUpdateBase('{"updateBase":"  "}'), null);
  assert.equal(readUpdateBase(''), null);
  assert.equal(readUpdateBase('{'), null);
});
