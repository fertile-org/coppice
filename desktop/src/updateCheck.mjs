const SEMVER_RE = /^v?(\d+)\.(\d+)\.(\d+)(?:-rc\.(\d+))?$/;

// Fallback only. The small `updates/latest.json` on the update host is the
// normal source, so this list stays short.
const RELEASES_PER_PAGE = 10;

function parse(version) {
  const match = SEMVER_RE.exec(String(version).trim());
  if (!match) return null;
  const [, major, minor, patch, rc] = match;
  return {
    core: [Number(major), Number(minor), Number(patch)],
    rc: rc === undefined ? null : Number(rc),
  };
}

export function isNewer(latestTag, current) {
  const a = parse(latestTag);
  const b = parse(current);
  if (!a || !b) return false;
  for (let i = 0; i < 3; i += 1) {
    if (a.core[i] !== b.core[i]) return a.core[i] > b.core[i];
  }
  if (a.rc === b.rc) return false;
  if (a.rc === null) return true;
  if (b.rc === null) return false;
  return a.rc > b.rc;
}

export function versionFromTag(tag) {
  return String(tag).replace(/^v/, '');
}

// Newest published release, prereleases included. Drafts are skipped.
// `releases/latest` hides prereleases, so the list is required.
export function selectNewerRelease(releases, currentVersion) {
  if (!Array.isArray(releases)) return null;
  let best = null;
  for (const release of releases) {
    if (!release || release.draft === true) continue;
    const tag = release.tag_name;
    const url = release.html_url;
    if (typeof tag !== 'string' || typeof url !== 'string') continue;
    if (!isNewer(tag, currentVersion)) continue;
    if (!best || isNewer(tag, best.tag)) best = { tag, url };
  }
  if (!best) return null;
  return { version: versionFromTag(best.tag), url: best.url };
}

export function releasesUrl(repo) {
  return `https://api.github.com/repos/${repo}/releases?per_page=${RELEASES_PER_PAGE}`;
}

export function latestManifestUrl(updateBase) {
  return `${String(updateBase).replace(/\/+$/, '')}/updates/latest.json`;
}

// `updates/latest.json` as written by scripts/latest-json.mjs. Anything that
// does not look like it is ignored, and the GitHub list is used instead.
export function readLatestManifest(body) {
  if (!body || typeof body !== 'object') return null;
  const { tag, url } = body;
  if (typeof tag !== 'string' || typeof url !== 'string' || !SEMVER_RE.test(tag)) return null;
  if (!url.startsWith('https://')) return null;
  return { tag, url };
}

function compare(latest, currentVersion) {
  if (!isNewer(latest.tag, currentVersion)) return { status: 'up-to-date' };
  return { status: 'available', version: versionFromTag(latest.tag), url: latest.url };
}

async function fromManifest(updateBase, currentVersion, fetchImpl) {
  const res = await fetchImpl(latestManifestUrl(updateBase), {
    headers: { Accept: 'application/json', 'User-Agent': 'coppice' },
    signal: AbortSignal.timeout(10_000),
  });
  if (!res.ok) return null;
  const latest = readLatestManifest(await res.json());
  return latest ? compare(latest, currentVersion) : null;
}

async function fromReleaseList(repo, currentVersion, fetchImpl) {
  const res = await fetchImpl(releasesUrl(repo), {
    headers: {
      Accept: 'application/vnd.github+json',
      'User-Agent': 'coppice',
    },
    signal: AbortSignal.timeout(10_000),
  });
  if (!res.ok) return { status: 'error' };
  const body = await res.json();
  const newer = selectNewerRelease(body, currentVersion);
  if (!newer) return { status: 'up-to-date' };
  return { status: 'available', version: newer.version, url: newer.url };
}

// Reads `updateBase/updates/latest.json` when a base is given, and falls back
// to a short GitHub release list when that file can't be read.
export async function checkForUpdate({ repo, currentVersion, updateBase = null, fetchImpl = fetch }) {
  if (updateBase) {
    try {
      const result = await fromManifest(updateBase, currentVersion, fetchImpl);
      if (result) return result;
    } catch {
      // Fall through to the release list.
    }
  }
  try {
    return await fromReleaseList(repo, currentVersion, fetchImpl);
  } catch {
    return { status: 'error' };
  }
}
