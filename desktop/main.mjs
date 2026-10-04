import { app, BrowserWindow, dialog, ipcMain, shell } from 'electron';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { startServer } from './src/serverProcess.mjs';
import { resolveLoginShellPath } from './src/shellPath.mjs';
import { checkForUpdate } from './src/updateCheck.mjs';
import { devUserDataPath } from './src/userDataPath.mjs';
import { createErrorWindow, createMainWindow, createSplashWindow } from './src/windows.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DEV_URL = process.env.COPPICE_WEB_URL;
const UPDATE_INTERVAL_MS = 24 * 60 * 60 * 1000;
const RELEASE_REPO = JSON.parse(fs.readFileSync(path.join(__dirname, 'package.json'), 'utf8'))
  .coppice.releaseRepo;

const devUserData = devUserDataPath({
  isPackaged: app.isPackaged,
  env: process.env,
  appData: app.getPath('appData'),
});
if (devUserData) app.setPath('userData', devUserData);

let mainWindow = null;
let splashWindow = null;
let errorWindow = null;
let errorDetails = null;
let server = null;
let serverUrl = null;
let quitting = false;
let readyToQuit = false;
let updatePromise = Promise.resolve(null);
let updateTimer = null;

function resourcesDir() {
  return app.isPackaged ? process.resourcesPath : path.join(__dirname, 'resources');
}

function logsDir() {
  return path.join(app.getPath('userData'), 'logs');
}

ipcMain.handle('coppice:pick-directory', async (event) => {
  const win = BrowserWindow.fromWebContents(event.sender);
  const result = await dialog.showOpenDialog(win ?? undefined, {
    properties: ['openDirectory'],
  });
  if (result.canceled || result.filePaths.length === 0) {
    return null;
  }
  return result.filePaths[0] ?? null;
});

ipcMain.handle('coppice:app-info', () => ({
  version: app.getVersion(),
  platform: process.platform,
  arch: process.arch,
}));

ipcMain.handle('coppice:update-info', () => updatePromise);

ipcMain.handle('coppice:error-details', (event) => {
  if (event.sender !== errorWindow?.webContents) return null;
  return errorDetails;
});

ipcMain.handle('coppice:error-action', (event, action) => {
  if (event.sender !== errorWindow?.webContents) return;
  if (action === 'open-logs') {
    void shell.openPath(logsDir());
  } else if (action === 'retry') {
    void ensureLaunch();
  } else if (action === 'quit') {
    app.quit();
  }
});

function startUpdateChecks() {
  if (updateTimer) return;
  const check = () => {
    updatePromise = checkForUpdate({ repo: RELEASE_REPO, currentVersion: app.getVersion() });
  };
  check();
  updateTimer = setInterval(check, UPDATE_INTERVAL_MS);
  updateTimer.unref?.();
}

function stopUpdateChecks() {
  if (updateTimer) clearInterval(updateTimer);
  updateTimer = null;
}

function closeWindow(win) {
  if (win && !win.isDestroyed()) win.close();
}

/** Opens the new window before closing the old ones so `window-all-closed` never fires in between. */
function showError(message, lines) {
  errorDetails = { message, lines };
  const previous = [splashWindow, mainWindow, errorWindow];
  errorWindow = createErrorWindow();
  splashWindow = null;
  mainWindow = null;
  previous.forEach(closeWindow);
}

function openMainWindow(url) {
  const win = createMainWindow({ origin: new URL(url).origin });
  mainWindow = win;
  win.once('ready-to-show', () => {
    closeWindow(splashWindow);
    splashWindow = null;
  });
  win.on('closed', () => {
    if (mainWindow === win) mainWindow = null;
  });
  void win.loadURL(url);
}

let shellPathPromise = null;
let launching = null;

function showSplash() {
  if (!splashWindow || splashWindow.isDestroyed()) {
    splashWindow = createSplashWindow();
  } else {
    splashWindow.focus();
  }
}

/** At most one launch runs at a time; two servers must never share the data dir. */
function ensureLaunch() {
  if (launching) {
    showSplash();
  } else {
    launching = launchServer().finally(() => {
      launching = null;
    });
  }
  return launching;
}

async function launchServer() {
  const previousError = errorWindow;
  errorWindow = null;
  showSplash();
  closeWindow(previousError);

  let current = null;
  let url;
  try {
    shellPathPromise ??= resolveLoginShellPath();
    const PATH = await shellPathPromise;
    if (quitting) return;

    const resources = resourcesDir();
    if (server) void server.stop();
    current = startServer({
      command: path.join(resources, 'bin', 'coppice-server'),
      args: ['desktop', '--data-dir', app.getPath('userData'), '--resources', resources],
      env: { ...process.env, PATH },
      logFile: path.join(logsDir(), 'server.log'),
    });
    server = current;
    url = await current.ready;
  } catch (err) {
    if (current) await current.stop();
    if (current && current !== server) return;
    if (!quitting) showError(err.message, current?.lastLines(50) ?? []);
    return;
  }
  if (current !== server) {
    void current.stop();
    return;
  }
  if (quitting) return;

  serverUrl = url;
  openMainWindow(url);
  startUpdateChecks();

  void current.exited.then((code) => {
    if (current !== server) {
      void current.stop();
      return;
    }
    if (quitting) return;
    serverUrl = null;
    void current.stop();
    showError(`Coppice server stopped unexpectedly (exit code ${code}).`, current.lastLines(50));
  });
}

function startDevMode() {
  mainWindow = createMainWindow({ origin: new URL(DEV_URL).origin });
  void mainWindow.loadURL(DEV_URL);
  mainWindow.on('closed', () => {
    mainWindow = null;
  });
}

function focusExistingWindow() {
  const win = mainWindow ?? errorWindow ?? splashWindow;
  if (!win || win.isDestroyed()) return;
  if (win.isMinimized()) win.restore();
  win.focus();
}

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

app.on('before-quit', (event) => {
  quitting = true;
  if (readyToQuit || !server) return;
  event.preventDefault();
  stopUpdateChecks();
  void server.stop().finally(() => {
    readyToQuit = true;
    app.quit();
  });
});

if (DEV_URL) {
  app.whenReady().then(() => {
    startDevMode();
    app.on('activate', () => {
      if (BrowserWindow.getAllWindows().length === 0) startDevMode();
    });
  });
} else if (!app.requestSingleInstanceLock()) {
  app.quit();
} else {
  app.on('second-instance', focusExistingWindow);
  app.whenReady().then(() => {
    void ensureLaunch();
    app.on('activate', () => {
      if (BrowserWindow.getAllWindows().length > 0) return;
      if (serverUrl && !launching) {
        openMainWindow(serverUrl);
      } else {
        void ensureLaunch();
      }
    });
  });
}
