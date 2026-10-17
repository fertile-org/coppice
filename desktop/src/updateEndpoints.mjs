// The only copy of the default update host. Override it with the
// COPPICE_UPDATE_BASE repository variable (passed through at package time).
// The value must be an https origin: a host, no credentials, no path, no
// query. Packaging calls this and the build fails when the value is rejected.
export const DEFAULT_UPDATE_BASE = 'https://apt.getcoppice.dev';

function rejectUpdateBase(detail) {
  throw new Error(`COPPICE_UPDATE_BASE must be an https origin with no path: ${detail}`);
}

// Path from the raw string, so "." and ".." segments the URL parser would
// fold away still count as a path.
function rawPath(candidate) {
  const scheme = candidate.match(/^([a-zA-Z][a-zA-Z+\-.]*):/);
  if (!scheme) return candidate;
  let rest = candidate.slice(scheme[0].length);
  if (rest.startsWith('//')) rest = rest.slice(2);
  const at = rest.lastIndexOf('@');
  if (at !== -1) rest = rest.slice(at + 1);
  const cut = rest.search(/[/?#]/);
  return cut === -1 ? '' : rest.slice(cut);
}

export function updateBase(value) {
  const raw = typeof value === 'string' ? value.trim() : '';
  const candidate = raw || DEFAULT_UPDATE_BASE;
  let url;
  try {
    url = new URL(candidate);
  } catch {
    rejectUpdateBase(JSON.stringify(candidate));
  }
  if (url.protocol !== 'https:') rejectUpdateBase('it must start with https://');
  if (!url.hostname) rejectUpdateBase('it needs a host');
  if (url.username || url.password) rejectUpdateBase('it must not include credentials');
  if (url.search || url.hash) rejectUpdateBase('it must not include a query or fragment');
  const path = rawPath(candidate);
  if (path !== '' && !/^\/+$/.test(path)) rejectUpdateBase(`it must not include a path (${path})`);
  return `https://${url.host}`;
}

// Trailing slash matters: electron-updater joins `latest-mac.yml` onto this URL.
export function macUpdateFeed(value) {
  return `${updateBase(value)}/updates/`;
}

// Hostname written to the Pages CNAME so the custom domain survives each deploy.
export function pagesCname(value) {
  return new URL(updateBase(value)).host;
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
