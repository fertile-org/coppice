// The only copy of the default update host. Override it with the
// COPPICE_UPDATE_BASE repository variable (passed through at package time).
// No trailing slash.
export const DEFAULT_UPDATE_BASE = 'https://fertile-org.github.io/coppice';

export function updateBase(value) {
  const raw = typeof value === 'string' ? value.trim() : '';
  return (raw || DEFAULT_UPDATE_BASE).replace(/\/+$/, '');
}

// Trailing slash matters: electron-updater joins `latest-mac.yml` onto this URL.
export function macUpdateFeed(value) {
  return `${updateBase(value)}/updates/`;
}

export function aptSourceLine(value) {
  return `deb [arch=amd64,arm64 signed-by=${APT_KEYRING_PATH}] ${updateBase(value)} ${APT_SUITE} ${APT_COMPONENT}`;
}

export const PAGES_BASE = DEFAULT_UPDATE_BASE;
export const MAC_UPDATE_FEED = macUpdateFeed();

export const APT_SUITE = 'stable';
export const APT_COMPONENT = 'main';
export const APT_KEYRING_PATH = '/usr/share/keyrings/coppice-archive-keyring.gpg';
export const APT_LIST_PATH = '/etc/apt/sources.list.d/coppice.list';
export const APT_SOURCE_LINE = aptSourceLine();
