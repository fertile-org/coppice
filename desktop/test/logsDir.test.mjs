import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { desktopServerArgs, resolveAppLogsDir } from '../src/logsDir.mjs';

test('resolveAppLogsDir follows XDG on linux and Library/Logs on macOS', () => {
  assert.equal(
    resolveAppLogsDir({
      platform: 'linux',
      env: { XDG_STATE_HOME: '/xdg/state' },
      home: '/home/u',
    }),
    path.join('/xdg/state', 'Coppice', 'logs'),
  );
  assert.equal(
    resolveAppLogsDir({ platform: 'linux', env: {}, home: '/home/u' }),
    path.join('/home/u', '.local', 'state', 'Coppice', 'logs'),
  );
  assert.equal(
    resolveAppLogsDir({ platform: 'linux', env: { XDG_STATE_HOME: '' }, home: '/home/u' }),
    path.join('/home/u', '.local', 'state', 'Coppice', 'logs'),
  );
  assert.equal(
    resolveAppLogsDir({ platform: 'darwin', env: { XDG_STATE_HOME: '/ignored' }, home: '/Users/u' }),
    path.join('/Users/u', 'Library', 'Logs', 'Coppice'),
  );

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
