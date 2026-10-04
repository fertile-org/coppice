import { app, BrowserWindow, shell } from 'electron';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { isExternalHttpUrl, isSameOrigin } from './navigation.mjs';

const DESKTOP_DIR = path.join(path.dirname(fileURLToPath(import.meta.url)), '..');
const STATIC_DIR = path.join(DESKTOP_DIR, 'static');
// Packaged Linux builds take the icon from the .desktop entry; build/ is not shipped.
const DEV_LINUX_ICON =
  process.platform === 'linux' && !app.isPackaged ? path.join(DESKTOP_DIR, 'build', 'icon.png') : undefined;

function openExternally(url) {
  if (isExternalHttpUrl(url)) {
    void shell.openExternal(url);
  }
}

/** Keeps `win` on `origin`; http(s) links elsewhere open in the system browser. */
export function restrictNavigation(win, origin) {
  const allowed = (url) => Boolean(origin) && isSameOrigin(url, origin);
  win.webContents.on('will-navigate', (event) => {
    if (allowed(event.url)) return;
    event.preventDefault();
    openExternally(event.url);
  });
  win.webContents.on('will-frame-navigate', (event) => {
    if (event.isMainFrame || allowed(event.url)) return;
    event.preventDefault();
  });
  win.webContents.on('will-redirect', (event) => {
    if (allowed(event.url)) return;
    event.preventDefault();
  });
  win.webContents.setWindowOpenHandler(({ url }) => {
    openExternally(url);
    return { action: 'deny' };
  });
}

export function createMainWindow({ origin }) {
  const win = new BrowserWindow({
    width: 1280,
    height: 840,
    show: false,
    icon: DEV_LINUX_ICON,
    webPreferences: {
      // CommonJS preload — Electron's ESM (.mjs) preload support is unreliable.
      preload: path.join(DESKTOP_DIR, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });
  win.webContents.on('preload-error', (_event, preloadPath, error) => {
    console.error('preload failed:', preloadPath, error);
  });
  restrictNavigation(win, origin);
  win.once('ready-to-show', () => win.show());
  return win;
}

export function createSplashWindow() {
  const win = new BrowserWindow({
    width: 360,
    height: 220,
    frame: false,
    resizable: false,
    show: false,
    webPreferences: { contextIsolation: true, nodeIntegration: false, sandbox: true },
  });
  restrictNavigation(win, null);
  win.once('ready-to-show', () => win.show());
  void win.loadFile(path.join(STATIC_DIR, 'splash.html'));
  return win;
}

export function createErrorWindow() {
  const win = new BrowserWindow({
    width: 760,
    height: 520,
    show: false,
    icon: DEV_LINUX_ICON,
    title: 'Coppice could not start',
    webPreferences: {
      preload: path.join(STATIC_DIR, 'error-preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });
  win.setMenuBarVisibility(false);
  restrictNavigation(win, null);
  win.once('ready-to-show', () => win.show());
  void win.loadFile(path.join(STATIC_DIR, 'error.html'));
  return win;
}
