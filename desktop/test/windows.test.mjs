import test from 'node:test';
import assert from 'node:assert/strict';
import { disposeWindow } from '../src/disposeWindow.mjs';

function fakeWindow() {
  let destroyed = false;
  return {
    isDestroyed: () => destroyed,
    destroy() {
      destroyed = true;
    },
    close() {
      throw new Error('use destroy(), not close() — splash/error must free the renderer now');
    },
  };
}

test('disposeWindow destroys a live window so splash cannot linger hidden', () => {
  const win = fakeWindow();
  disposeWindow(win);
  assert.equal(win.isDestroyed(), true);
});

test('disposeWindow is a no-op for missing or already-destroyed windows', () => {
  assert.doesNotThrow(() => disposeWindow(null));
  assert.doesNotThrow(() => disposeWindow(undefined));
  const win = fakeWindow();
  win.destroy();
  assert.doesNotThrow(() => disposeWindow(win));
});
