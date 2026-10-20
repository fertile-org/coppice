import { execFile } from 'node:child_process';

import { isNewer } from './updateCheck.mjs';

// Matches `deb.packageName` in electron-builder.yml.
export const DEB_PACKAGE = 'coppice-desktop';
export const INSTALL_POLL_MS = 10 * 1000;

// The control Version is the app version with `-rc.` written as `~rc.`
// (scripts/release-version.mjs). Anything else is not ours and reads as null.
export function appVersionFromDeb(debVersion) {
  const version = String(debVersion ?? '').trim().replace(/~rc\./, '-rc.');
  return /^\d+\.\d+\.\d+(?:-rc\.\d+)?$/.test(version) ? version : null;
}

// Version dpkg has on disk, or null when dpkg-query fails (not a deb install).
export function readInstalledVersion(execFileImpl = execFile) {
  return new Promise((resolve) => {
    execFileImpl(
      'dpkg-query',
      ['-W', '-f=${Version}', DEB_PACKAGE],
      { timeout: 5000 },
      (error, stdout) => {
        resolve(error ? null : appVersionFromDeb(stdout));
      },
    );
  });
}

// Polls until apt has put a newer package on disk than the one running, then
// calls onInstalled once with that version.
export function watchLinuxInstall({
  currentVersion,
  read = readInstalledVersion,
  onInstalled,
  intervalMs = INSTALL_POLL_MS,
  setIntervalImpl = setInterval,
  clearIntervalImpl = clearInterval,
}) {
  let stopped = false;
  const tick = async () => {
    const installed = await read();
    if (stopped || !installed || !isNewer(installed, currentVersion)) return;
    stop();
    onInstalled(installed);
  };
  const timer = setIntervalImpl(() => {
    void tick();
  }, intervalMs);
  timer.unref?.();
  function stop() {
    stopped = true;
    clearIntervalImpl(timer);
  }
  void tick();
  return { stop };
}
