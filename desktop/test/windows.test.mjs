import test from 'node:test';
import assert from 'node:assert/strict';
import { disposeWindow } from '../src/disposeWindow.mjs';
import {
  attachMainWindowFocus,
  focusBrowserWindow,
  restoreWebContentsFocus,
} from '../src/windowFocus.mjs';

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

function fakeFocusedWindow() {
  const listeners = new Map();
  let destroyed = false;
  let minimized = false;
  const webContents = { focusCalls: 0, focus() { this.focusCalls += 1; } };
  return {
    webContents,
    focusCalls: 0,
    restoreCalls: 0,
    isDestroyed: () => destroyed,
    isMinimized: () => minimized,
    destroy() {
      destroyed = true;
    },
    minimize() {
      minimized = true;
    },
    restore() {
      minimized = false;
      this.restoreCalls += 1;
    },
    focus() {
      this.focusCalls += 1;
    },
    on(event, handler) {
      listeners.set(event, handler);
    },
    emit(event) {
      listeners.get(event)?.();
    },
  };
}

test('restoreWebContentsFocus gives the renderer keyboard focus', () => {
  const win = fakeFocusedWindow();
  restoreWebContentsFocus(win);
  assert.equal(win.webContents.focusCalls, 1);
});

test('restoreWebContentsFocus is a no-op for missing or destroyed windows', () => {
  assert.doesNotThrow(() => restoreWebContentsFocus(null));
  const win = fakeFocusedWindow();
  win.destroy();
  restoreWebContentsFocus(win);
  assert.equal(win.webContents.focusCalls, 0);
});

test('attachMainWindowFocus restores webContents when the OS window is focused', () => {
  const win = fakeFocusedWindow();
  attachMainWindowFocus(win);
  win.emit('focus');
  assert.equal(win.webContents.focusCalls, 1);
});

test('focusBrowserWindow restores a minimized window and focuses webContents', () => {
  const win = fakeFocusedWindow();
  win.minimize();
  focusBrowserWindow(win);
  assert.equal(win.restoreCalls, 1);
  assert.equal(win.focusCalls, 1);
  assert.equal(win.webContents.focusCalls, 1);
});
