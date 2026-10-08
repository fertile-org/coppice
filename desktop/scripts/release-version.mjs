#!/usr/bin/env node
// Validates a release tag (vX.Y.Z or vX.Y.Z-rc.N), writes its version into
// desktop/package.json and prints `version=` / `prerelease=` lines for
// $GITHUB_OUTPUT. The rewritten package.json is a build input, never committed.
//
//   node scripts/release-version.mjs <tag>

import { readFile, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const desktopDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const NUM = '(0|[1-9]\\d*)';
const TAG_RE = new RegExp(`^v(${NUM}\\.${NUM}\\.${NUM}(-rc\\.${NUM})?)$`);

export function parseReleaseTag(tag) {
  const match = typeof tag === 'string' ? TAG_RE.exec(tag) : null;
  if (!match) {
    throw new Error(`invalid release tag ${JSON.stringify(tag)}: expected vX.Y.Z or vX.Y.Z-rc.N`);
  }
  return { version: match[1], prerelease: match[5] !== undefined };
}

// Debian splits on the last hyphen, so 0.1.0-rc.7 is upstream 0.1.0 with
// revision rc.7 and sorts AFTER the final 0.1.0. A tilde sorts before the
// end of the upstream version, so 0.1.0~rc.7 < 0.1.0. The app version, git
// tag, and installer file name keep the hyphen.
const APP_VERSION_RE = new RegExp(`^${NUM}\\.${NUM}\\.${NUM}(-rc\\.${NUM})?$`);

export function debianPackageVersion(appVersion) {
  if (typeof appVersion !== 'string' || !APP_VERSION_RE.test(appVersion)) {
    throw new Error(`invalid app version ${JSON.stringify(appVersion)}: expected X.Y.Z or X.Y.Z-rc.N`);
  }
  return appVersion.replace(/-rc\./, '~rc.');
}

export async function setPackageVersion(file, version) {
  const pkg = JSON.parse(await readFile(file, 'utf8'));
  pkg.version = version;
  await writeFile(file, `${JSON.stringify(pkg, null, 2)}\n`);
}

async function main() {
  const args = process.argv.slice(2);
  if (args.length !== 1) {
    console.error('usage: release-version.mjs <tag>');
    process.exit(2);
  }
  let parsed;
  try {
    parsed = parseReleaseTag(args[0]);
  } catch (err) {
    console.error(err.message);
    process.exit(1);
  }
  await setPackageVersion(join(desktopDir, 'package.json'), parsed.version);
  console.log(`version=${parsed.version}`);
  console.log(`prerelease=${parsed.prerelease}`);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
