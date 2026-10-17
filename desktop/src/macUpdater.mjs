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

export function attachMacUpdater(autoUpdater, { onDownloaded, onError } = {}) {
  autoUpdater.on('update-downloaded', (info) => {
    onDownloaded?.({ version: typeof info?.version === 'string' ? info.version : '' });
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
