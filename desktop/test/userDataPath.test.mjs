import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { devUserDataPath } from '../src/userDataPath.mjs';

test('packaged apps keep the OS default', () => {
  assert.equal(
    devUserDataPath({ isPackaged: true, env: { COPPICE_DESKTOP_USER_DATA: '/tmp/x' }, appData: '/a' }),
    null,
  );
});

test('unpackaged runs default to <appData>/Coppice-dev', () => {
  assert.equal(devUserDataPath({ isPackaged: false, env: {}, appData: '/a' }), path.join('/a', 'Coppice-dev'));
  assert.equal(
    devUserDataPath({ isPackaged: false, env: { COPPICE_DESKTOP_USER_DATA: '' }, appData: '/a' }),
    path.join('/a', 'Coppice-dev'),
  );
});

test('unpackaged runs honour COPPICE_DESKTOP_USER_DATA', () => {
  assert.equal(
    devUserDataPath({ isPackaged: false, env: { COPPICE_DESKTOP_USER_DATA: '/tmp/x' }, appData: '/a' }),
    path.resolve('/tmp/x'),
  );
});
