export const PAGES_BASE = 'https://fertile-org.github.io/coppice';

// Trailing slash matters: electron-updater joins `latest-mac.yml` onto this URL.
export const MAC_UPDATE_FEED = `${PAGES_BASE}/updates/`;

export const APT_SUITE = 'stable';
export const APT_COMPONENT = 'main';
export const APT_KEYRING_PATH = '/usr/share/keyrings/coppice-archive-keyring.gpg';
export const APT_LIST_PATH = '/etc/apt/sources.list.d/coppice.list';
export const APT_SOURCE_LINE = `deb [arch=amd64,arm64 signed-by=${APT_KEYRING_PATH}] ${PAGES_BASE} ${APT_SUITE} ${APT_COMPONENT}`;
