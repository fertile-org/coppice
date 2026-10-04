const SEMVER_RE = /^v?(\d+)\.(\d+)\.(\d+)(?:-rc\.(\d+))?$/;

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

export async function checkForUpdate({ repo, currentVersion, fetchImpl = fetch }) {
  try {
    const res = await fetchImpl(`https://api.github.com/repos/${repo}/releases/latest`, {
      headers: { Accept: 'application/vnd.github+json' },
    });
    if (!res.ok) return null;
    const body = await res.json();
    const tag = body?.tag_name;
    const url = body?.html_url;
    if (typeof tag !== 'string' || typeof url !== 'string') return null;
    if (!isNewer(tag, currentVersion)) return null;
    return { version: tag.replace(/^v/, ''), url };
  } catch {
    return null;
  }
}
