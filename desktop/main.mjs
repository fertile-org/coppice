import { app, BrowserWindow, dialog, ipcMain, shell } from 'electron';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { desktopServerArgs, resolveAppLogsDir } from './src/logsDir.mjs';
import { startServer } from './src/serverProcess.mjs';
import { startupFailureMessage } from './src/startupError.mjs';
import { resolveLoginShellPath } from './src/shellPath.mjs';
import { attachMacUpdater, configureMacFeed } from './src/macUpdater.mjs';
import { macUpdateFeed, updateBase } from './src/updateEndpoints.mjs';
import { readUpdateBase, readUpdaterMode } from './src/readUpdaterMode.mjs';
import { checkForUpdate } from './src/updateCheck.mjs';
import { watchLinuxInstall } from './src/linuxInstall.mjs';
import { bannerForUpdate, startUpdateLoop, UPDATE_INTERVAL_MS } from './src/updatePlan.mjs';
import { readUpdateSettings, UPDATE_SETTINGS_NAME, writeUpdateSettings } from './src/updateSettings.mjs';
import { devUserDataPath } from './src/userDataPath.mjs';
import { disposeWindow } from './src/disposeWindow.mjs';
import { createErrorWindow, createMainWindow, createSplashWindow } from './src/windows.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DEV_URL = process.env.COPPICE_WEB_URL;
const RELEASE_REPO = JSON.parse(fs.readFileSync(path.join(__dirname, 'package.json'), 'utf8'))
  .coppice.releaseRepo;

function readPackagedUpdaterMode() {
  try {
    return fs.readFileSync(path.join(__dirname, 'updater-mode.json'), 'utf8');
  } catch {
    return '';
  }
}

function packagedMacInPlaceUpdates() {
  if (process.platform !== 'darwin' || !app.isPackaged) return false;
  return readUpdaterMode(readPackagedUpdaterMode());
}

function packagedMacFeed() {
  return macUpdateFeed(readUpdateBase(readPackagedUpdaterMode()));
}

const devUserData = devUserDataPath({
  isPackaged: app.isPackaged,
  env: process.env,
  appData: app.getPath('appData'),
});
if (devUserData) app.setPath('userData', devUserData);

const appLogsDir = resolveAppLogsDir({
  platform: process.platform,
  env: process.env,
  home: app.getPath('home'),
});
app.setAppLogsPath(appLogsDir);

let mainWindow = null;
let splashWindow = null;
let errorWindow = null;
let errorDetails = null;
let server = null;
let serverUrl = null;
let quitting = false;
let readyToQuit = false;
let updateLoop = null;
let updateState = null;
let updateAvailability = null;
let downloadState = null;
let macUpdater = null;
let macUpdaterLoading = null;
let macInPlaceReady = packagedMacInPlaceUpdates();
let installingUpdate = false;
let linuxInstalled = null;
let linuxInstallWatch = null;
let checkGeneration = 0;

function resourcesDir() {
  return app.isPackaged ? process.resourcesPath : path.join(__dirname, 'resources');
}

function logsDir() {
  return app.getPath('logs');
}

ipcMain.handle('coppice:show-item-in-folder', (_event, filePath) => {
  if (typeof filePath !== 'string' || filePath.length === 0) return;
  shell.showItemInFolder(filePath);
});

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
  macInPlaceUpdates: macInPlaceReady,
}));

function settingsPath() {
  return path.join(app.getPath('userData'), UPDATE_SETTINGS_NAME);
}

function automaticUpdatesEnabled() {
  return readUpdateSettings(settingsPath()).automaticUpdates;
}

function broadcastUpdate(state) {
  updateState = state;
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('coppice:update-info', updateState);
  }
}

function publishBanner() {
  broadcastUpdate(
    bannerForUpdate({
      platform: process.platform,
      macInPlaceUpdates: macInPlaceReady,
      availability: updateAvailability,
      download: downloadState,
      linuxInstalled,
    }),
  );
}

// apt installs outside the app. While a newer release is out, watch dpkg so the
// banner can offer a restart once the package is on disk.
function syncLinuxInstallWatch() {
  if (process.platform !== 'linux' || !app.isPackaged) return;
  const want = updateAvailability !== null && linuxInstalled === null;
  if (want && !linuxInstallWatch) {
    linuxInstallWatch = watchLinuxInstall({
      currentVersion: app.getVersion(),
      onInstalled(version) {
        linuxInstallWatch = null;
        linuxInstalled = version;
        publishBanner();
      },
    });
  } else if (!want && linuxInstallWatch) {
    linuxInstallWatch.stop();
    linuxInstallWatch = null;
  }
}

async function ensureMacUpdater() {
  if (!macInPlaceReady) return;
  if (macUpdater) return;
  if (!macUpdaterLoading) {
    macUpdaterLoading = (async () => {
      try {
        const { autoUpdater } = await import('electron-updater');
        configureMacFeed(autoUpdater, packagedMacFeed());
        autoUpdater.autoInstallOnAppQuit = automaticUpdatesEnabled();
        macUpdater = attachMacUpdater(autoUpdater, {
          onDownloaded(info) {
            downloadState = { phase: 'downloaded', version: info.version };
            publishBanner();
          },
          onError() {
            if (downloadState?.phase === 'downloading') downloadState = null;
          },
        });
      } catch {
        macInPlaceReady = false;
        macUpdater = null;
      }
    })();
  }
  await macUpdaterLoading;
}

async function runUpdateCheck({ manual = false } = {}) {
  const generation = checkGeneration;
  await ensureMacUpdater();
  const result = await checkForUpdate({
    repo: RELEASE_REPO,
    currentVersion: app.getVersion(),
    updateBase: updateBase(readUpdateBase(readPackagedUpdaterMode())),
  });
  if (!manual && generation !== checkGeneration) return result;
  if (result.status === 'available') {
    updateAvailability = { version: result.version, url: result.url };
    if (macInPlaceReady && macUpdater) {
      const already = downloadState?.phase === 'downloaded' && downloadState.version === result.version;
      if (!already) {
        downloadState = { phase: 'downloading', version: result.version };
        void macUpdater.check().catch(() => {});
      }
    }
    publishBanner();
  } else if (result.status === 'up-to-date' && downloadState?.phase !== 'downloaded') {
    updateAvailability = null;
    downloadState = null;
    publishBanner();
  }
  syncLinuxInstallWatch();
  return result;
}

function startUpdateChecks() {
  if (updateLoop || !automaticUpdatesEnabled()) return;
  updateLoop = startUpdateLoop({ enabled: true, check: () => runUpdateCheck(), intervalMs: UPDATE_INTERVAL_MS });
}

function stopUpdateChecks() {
  updateLoop?.stop();
  updateLoop = null;
  checkGeneration += 1;
  linuxInstallWatch?.stop();
  linuxInstallWatch = null;
}

ipcMain.handle('coppice:update-info', () => updateState);

ipcMain.handle('coppice:update-settings-get', () => readUpdateSettings(settingsPath()));

ipcMain.handle('coppice:update-settings-set', (_event, value) => {
  const saved = writeUpdateSettings(settingsPath(), {
    automaticUpdates: value?.automaticUpdates === true,
  });
  macUpdater?.setAutoInstallOnQuit(saved.automaticUpdates);
  if (saved.automaticUpdates) startUpdateChecks();
  else {
    stopUpdateChecks();
    broadcastUpdate(null);
  }
  return saved;
});

ipcMain.handle('coppice:check-for-updates', async () => {
  const result = await runUpdateCheck({ manual: true });
  if (result.status === 'available') return { status: 'available', version: result.version };
  if (result.status === 'up-to-date') return { status: 'up-to-date' };
  return { status: 'error' };
});

ipcMain.handle('coppice:restart-to-update', async () => {
  if (process.platform === 'linux') {
    // apt already replaced the files; a relaunch runs the new version.
    if (!linuxInstalled) return;
    app.relaunch();
    app.quit();
    return;
  }
  if (!macUpdater || downloadState?.phase !== 'downloaded') return;
  quitting = true;
  stopUpdateChecks();
  if (server) {
    const current = server;
    server = null;
    await current.stop();
  }
  readyToQuit = true;
  installingUpdate = true;
  macUpdater.install();
});

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


/** Opens the new window before closing the old ones so `window-all-closed` never fires in between. */
function showError(message, lines) {
  errorDetails = { message, lines };
  const previous = [splashWindow, mainWindow, errorWindow];
  errorWindow = createErrorWindow();
  splashWindow = null;
  mainWindow = null;
  previous.forEach(disposeWindow);
}

function openMainWindow(url) {
  const win = createMainWindow({ origin: new URL(url).origin });
  mainWindow = win;
  win.once('ready-to-show', () => {
    disposeWindow(splashWindow);
    splashWindow = null;
  });
  win.on('closed', () => {
    if (mainWindow === win) mainWindow = null;
  });
  void win.loadURL(url);
}

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
  disposeWindow(previousError);

  let current = null;
  let url;
  try {
    const PATH = await resolveLoginShellPath();
    if (quitting) return;

    const resources = resourcesDir();
    if (server) void server.stop();
    current = startServer({
      command: path.join(resources, 'bin', 'coppice-server'),
      args: desktopServerArgs({
        userData: app.getPath('userData'),
        resources,
        logsDir: appLogsDir,
      }),
      env: { ...process.env, PATH },
    });
    server = current;
    url = await current.ready;
  } catch (err) {
    if (current) await current.stop();
    if (current && current !== server) return;
    const lines = current?.lastLines(50) ?? [];
    if (!quitting) showError(startupFailureMessage(lines, err.message), lines);
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

function installDownloadedUpdate() {
  if (!macUpdater || downloadState?.phase !== 'downloaded' || installingUpdate) return;
  installingUpdate = true;
  readyToQuit = true;
  macUpdater.install();
}

app.on('before-quit', (event) => {
  quitting = true;
  if (installingUpdate || readyToQuit) return;
  const installOnQuit = downloadState?.phase === 'downloaded' && automaticUpdatesEnabled();
  if (!server) {
    if (installOnQuit) {
      event.preventDefault();
      stopUpdateChecks();
      installDownloadedUpdate();
    }
    return;
  }
  event.preventDefault();
  stopUpdateChecks();
  const current = server;
  server = null;
  void current.stop().finally(() => {
    if (installOnQuit) {
      installDownloadedUpdate();
      return;
    }
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
