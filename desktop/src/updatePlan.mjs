import { DOWNLOAD_PAGE_URL } from './updateEndpoints.mjs';

export const UPDATE_INTERVAL_MS = 4 * 60 * 60 * 1000;

// Signed macOS hides the banner until the zip is on disk, unless the in-place
// path failed (Download) or the install cannot update in place (Move).
// Downloading is not a banner. On Linux, a newer package already installed by
// apt is ready to restart into.
export function bannerForUpdate({
  platform,
  macInPlaceUpdates,
  availability,
  download,
  linuxInstalled,
  installBlocked,
}) {
  if (platform === 'linux' && linuxInstalled) return { kind: 'ready', version: linuxInstalled };
  if (platform === 'darwin' && installBlocked && availability?.version) {
    return { kind: 'move', version: availability.version };
  }
  if (platform === 'darwin' && macInPlaceUpdates) {
    if (download?.phase === 'downloaded' && download.version) {
      return { kind: 'ready', version: download.version };
    }
    if (download?.phase === 'failed' && availability?.version) {
      return { kind: 'download', version: availability.version, url: DOWNLOAD_PAGE_URL };
    }
    return null;
  }
  if (!availability?.version) return null;
  if (platform === 'linux') return { kind: 'linux', version: availability.version };
  if (platform === 'darwin') {
    return { kind: 'download', version: availability.version, url: DOWNLOAD_PAGE_URL };
  }
  return null;
}

/** Map in-place download state to the Settings → Updates check result. */
export function macInPlaceCheckResult({ download, installBlocked, latest }) {
  if (installBlocked && (download || latest?.status === 'available')) {
    return {
      status: 'failed',
      reason: 'Move Coppice to the Applications folder to update in place.',
    };
  }
  if (download?.phase === 'downloaded' && download.version) {
    return { status: 'ready', version: download.version };
  }
  if (download?.phase === 'downloading' && download.version) {
    return { status: 'downloading', version: download.version };
  }
  if (download?.phase === 'failed') {
    return {
      status: 'failed',
      reason: download.reason || 'Update download failed.',
    };
  }
  if (latest?.status === 'available') return { status: 'available', version: latest.version };
  if (latest?.status === 'up-to-date') return { status: 'up-to-date' };
  return { status: 'error' };
}

// Starts the check and returns. The check's promise is not awaited.
export function startUpdateLoop({
  enabled,
  check,
  intervalMs = UPDATE_INTERVAL_MS,
  setIntervalImpl = setInterval,
  clearIntervalImpl = clearInterval,
}) {
  if (!enabled) return { stop() {} };
  const timer = setIntervalImpl(() => {
    void check();
  }, intervalMs);
  timer.unref?.();
  void check();
  return {
    stop() {
      clearIntervalImpl(timer);
    },
  };
}
