import test from 'node:test';
import assert from 'node:assert/strict';
import {
  APP_PASSWORD_RESET_FAILED,
  DATABASE_STILL_RUNNING,
  startupFailureMessage,
} from '../src/startupError.mjs';

const FALLBACK = 'coppice-server exited with code 1 before it was ready';

test('startupFailureMessage prefers desktop error lines over the process fallback', () => {
  assert.equal(
    startupFailureMessage(
      [
        'WARN could not read the postmaster command line; leaving it running',
        `coppice-server desktop: ${DATABASE_STILL_RUNNING}`,
      ],
      FALLBACK,
    ),
    DATABASE_STILL_RUNNING,
  );
  assert.equal(
    startupFailureMessage(
      ['INFO postgres started', `coppice-server desktop: ${APP_PASSWORD_RESET_FAILED}`],
      FALLBACK,
    ),
    APP_PASSWORD_RESET_FAILED,
  );
  assert.equal(startupFailureMessage(['initdb: error'], FALLBACK), FALLBACK);
  assert.equal(startupFailureMessage([], FALLBACK), FALLBACK);
});
