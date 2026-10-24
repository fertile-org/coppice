/** Give the renderer keyboard focus after a native dialog or OS focus change. */
export function restoreWebContentsFocus(win) {
  if (!win || win.isDestroyed?.()) return;
  win.webContents?.focus();
}

export function attachMainWindowFocus(win) {
  win.on('focus', () => restoreWebContentsFocus(win));
}

export function focusBrowserWindow(win) {
  if (!win || win.isDestroyed()) return;
  if (win.isMinimized()) win.restore();
  win.focus();
  restoreWebContentsFocus(win);
}
