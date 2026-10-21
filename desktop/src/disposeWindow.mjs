/** Drop a BrowserWindow immediately. `close()` can linger; splash and error
 * windows must not stay as hidden renderers after the next window is up. */
export function disposeWindow(win) {
  if (win && !win.isDestroyed()) win.destroy();
}
