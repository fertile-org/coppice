import { MAC_UPDATE_FEED } from './updateEndpoints.mjs';

export function configureMacFeed(autoUpdater, feed = MAC_UPDATE_FEED) {
  autoUpdater.setFeedURL({ provider: 'generic', url: feed });
  // Generic provider reads latest-mac.yml. Leaving channel unset keeps that
  // name; setting it would also flip allowDowngrade on.
  autoUpdater.allowPrerelease = true;
  autoUpdater.allowDowngrade = false;
  autoUpdater.autoDownload = true;
  autoUpdater.autoInstallOnAppQuit = true;
}

// Skip download-progress: it floods updater.log once per tick.
const LOGGED_EVENTS = [
  'checking-for-update',
  'update-available',
  'update-not-available',
  'update-downloaded',
  'error',
];

export const IN_PLACE_NOT_AVAILABLE_REASON =
  'Update feed has no newer build for this app (latest-mac.yml may lag latest.json).';

function summarizeEvent(name, payload) {
  if (name === 'error') {
    const err = payload;
    const detail = err?.message || err?.stack || String(err);
    return `error: ${detail}`;
  }
  if (name === 'update-available' || name === 'update-downloaded' || name === 'update-not-available') {
    const version = typeof payload?.version === 'string' ? payload.version : '';
    return version ? `${name}: ${version}` : name;
  }
  return name;
}

export function attachMacUpdater(autoUpdater, { onDownloaded, onError, onNotAvailable, onLog } = {}) {
  for (const name of LOGGED_EVENTS) {
    autoUpdater.on(name, (payload) => {
      onLog?.(summarizeEvent(name, payload));
    });
  }
  autoUpdater.on('update-downloaded', (info) => {
    onDownloaded?.({ version: typeof info?.version === 'string' ? info.version : '' });
  });
  autoUpdater.on('update-not-available', (info) => {
    onNotAvailable?.(info);
  });
  autoUpdater.on('error', (error) => {
    onError?.(error);
  });
  return {
    check() {
      return autoUpdater.checkForUpdates();
    },
    install() {
      autoUpdater.quitAndInstall(false, true);
    },
    setAutoInstallOnQuit(enabled) {
      autoUpdater.autoInstallOnAppQuit = enabled === true;
    },
  };
}
