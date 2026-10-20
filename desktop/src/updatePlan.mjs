export const UPDATE_INTERVAL_MS = 4 * 60 * 60 * 1000;

// Signed macOS hides the banner until the zip is on disk. Anything else
// shows as soon as a newer release exists. Downloading is not a banner.
// On Linux, a newer package already installed by apt is ready to restart into.
export function bannerForUpdate({ platform, macInPlaceUpdates, availability, download, linuxInstalled }) {
  if (platform === 'linux' && linuxInstalled) return { kind: 'ready', version: linuxInstalled };
  if (platform === 'darwin' && macInPlaceUpdates) {
    if (download?.phase === 'downloaded' && download.version) {
      return { kind: 'ready', version: download.version };
    }
    return null;
  }
  if (!availability?.version) return null;
  if (platform === 'linux') return { kind: 'linux', version: availability.version };
  if (platform === 'darwin') {
    return { kind: 'download', version: availability.version, url: availability.url };
  }
  return null;
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
