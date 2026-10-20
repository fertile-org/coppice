import assert from 'node:assert/strict';
import test from 'node:test';

import {
  DEB_PACKAGE,
  appVersionFromDeb,
  readInstalledVersion,
  watchLinuxInstall,
} from '../src/linuxInstall.mjs';

test('deb versions map back to app versions', () => {
  assert.equal(appVersionFromDeb('0.1.0~rc.10\n'), '0.1.0-rc.10');
  assert.equal(appVersionFromDeb('1.2.3'), '1.2.3');
  assert.equal(appVersionFromDeb(''), null);
  assert.equal(appVersionFromDeb('1:2.0-1ubuntu1'), null);
});

test('reads the installed version with dpkg-query', async () => {
  let call;
  const execFileImpl = (file, args, options, callback) => {
    call = { file, args };
    callback(null, '0.1.0~rc.10');
  };
  assert.equal(await readInstalledVersion(execFileImpl), '0.1.0-rc.10');
  assert.deepEqual(call, { file: 'dpkg-query', args: ['-W', '-f=${Version}', DEB_PACKAGE] });
});

test('a failed dpkg-query reads as not installed', async () => {
  const execFileImpl = (_file, _args, _options, callback) => callback(new Error('no dpkg'), '');
  assert.equal(await readInstalledVersion(execFileImpl), null);
});

function fakeTimers() {
  const timers = { tick: null, cleared: false };
  return {
    timers,
    setIntervalImpl: (fn) => {
      timers.tick = fn;
      return { unref() {} };
    },
    clearIntervalImpl: () => {
      timers.cleared = true;
    },
  };
}

test('reports a newer installed package once and stops polling', async () => {
  const versions = ['0.1.0-rc.9', '0.1.0-rc.10'];
  const installed = [];
  const { timers, setIntervalImpl, clearIntervalImpl } = fakeTimers();
  watchLinuxInstall({
    currentVersion: '0.1.0-rc.9',
    read: async () => versions.shift() ?? '0.1.0-rc.10',
    onInstalled: (version) => installed.push(version),
    setIntervalImpl,
    clearIntervalImpl,
  });
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(installed, []);
  timers.tick();
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(installed, ['0.1.0-rc.10']);
  assert.equal(timers.cleared, true);
});

test('stop ignores a read that finishes later', async () => {
  let finish;
  const installed = [];
  const { setIntervalImpl, clearIntervalImpl } = fakeTimers();
  const watch = watchLinuxInstall({
    currentVersion: '0.1.0-rc.9',
    read: () => new Promise((resolve) => (finish = resolve)),
    onInstalled: (version) => installed.push(version),
    setIntervalImpl,
    clearIntervalImpl,
  });
  watch.stop();
  finish('0.1.0-rc.10');
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(installed, []);
});
