import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { desktopServerArgs, resolveAppLogsDir } from '../src/logsDir.mjs';

test('linux uses XDG_STATE_HOME when it is set and non-empty', () => {
  assert.equal(
    resolveAppLogsDir({
      platform: 'linux',
      env: { XDG_STATE_HOME: '/xdg/state' },
      home: '/home/u',
    }),
    path.join('/xdg/state', 'Coppice', 'logs'),
  );
});

test('linux uses ~/.local/state/Coppice/logs when XDG_STATE_HOME is unset', () => {
  assert.equal(
    resolveAppLogsDir({ platform: 'linux', env: {}, home: '/home/u' }),
    path.join('/home/u', '.local', 'state', 'Coppice', 'logs'),
  );
});

test('linux uses ~/.local/state/Coppice/logs when XDG_STATE_HOME is empty', () => {
  assert.equal(
    resolveAppLogsDir({ platform: 'linux', env: { XDG_STATE_HOME: '' }, home: '/home/u' }),
    path.join('/home/u', '.local', 'state', 'Coppice', 'logs'),
  );
});

test('macOS resolves ~/Library/Logs/Coppice', () => {
  assert.equal(
    resolveAppLogsDir({ platform: 'darwin', env: { XDG_STATE_HOME: '/ignored' }, home: '/Users/u' }),
    path.join('/Users/u', 'Library', 'Logs', 'Coppice'),
  );
});

test('setAppLogsPath and --logs-dir receive the same directory', () => {
  const logsDir = resolveAppLogsDir({
    platform: 'linux',
    env: { XDG_STATE_HOME: '/xdg/state' },
    home: '/home/u',
  });
  const args = desktopServerArgs({
    userData: '/data',
    resources: '/res',
    logsDir,
  });
  assert.equal(args[args.indexOf('--logs-dir') + 1], logsDir);
});
