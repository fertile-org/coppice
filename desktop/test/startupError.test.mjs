import test from 'node:test';
import assert from 'node:assert/strict';
import {
  APP_PASSWORD_RESET_FAILED,
  DATABASE_STILL_RUNNING,
  startupFailureMessage,
} from '../src/startupError.mjs';

const FALLBACK = 'coppice-server exited with code 1 before it was ready';

test('a leftover database line becomes the error the window shows', () => {
  const lines = [
    'WARN could not read the postmaster command line; leaving it running',
    `coppice-server desktop: ${DATABASE_STILL_RUNNING}`,
  ];
  assert.equal(startupFailureMessage(lines, FALLBACK), DATABASE_STILL_RUNNING);
});

test('a lost database password becomes the error the window shows', () => {
  const lines = [
    'INFO postgres started',
    `coppice-server desktop: ${APP_PASSWORD_RESET_FAILED}`,
  ];
  assert.equal(startupFailureMessage(lines, FALLBACK), APP_PASSWORD_RESET_FAILED);
});

test('other startup failures keep the process message', () => {
  assert.equal(startupFailureMessage(['initdb: error'], FALLBACK), FALLBACK);
  assert.equal(startupFailureMessage([], FALLBACK), FALLBACK);
});
