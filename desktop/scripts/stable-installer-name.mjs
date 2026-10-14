#!/usr/bin/env node
// Stable download names for the four desktop installers.
//
// Versioned files keep electron-builder's name (`Coppice-<version>-…`).
// The release also uploads a copy without the version so
// releases/latest/download/<file> does not change between tags.
// A release-candidate version contains a hyphen (`0.1.0-rc.7`); the version
// is stripped as a prefix, not by splitting on `-`.
//
//   node scripts/stable-installer-name.mjs --list
//   node scripts/stable-installer-name.mjs <version> <versioned-file>

import { basename } from 'node:path';
import { pathToFileURL } from 'node:url';

export const STABLE_INSTALLER_NAMES = [
  'Coppice-linux-amd64.deb',
  'Coppice-linux-arm64.deb',
  'Coppice-mac-arm64.dmg',
  'Coppice-mac-x64.dmg',
];

export function stableInstallerName(version, versionedPath) {
  if (typeof version !== 'string' || version.length === 0 || /[\\/]/.test(version)) {
    throw new Error(`invalid version ${JSON.stringify(version)}`);
  }
  const base = basename(String(versionedPath));
  const prefix = `Coppice-${version}-`;
  if (!base.startsWith(prefix) || base.length === prefix.length) {
    throw new Error(`${base} is not named Coppice-${version}-<platform>`);
  }
  const stable = `Coppice-${base.slice(prefix.length)}`;
  if (!STABLE_INSTALLER_NAMES.includes(stable)) {
    throw new Error(
      `${stable} is not a stable installer name (${STABLE_INSTALLER_NAMES.join(', ')})`,
    );
  }
  return stable;
}

function main() {
  const args = process.argv.slice(2);
  if (args.length === 1 && args[0] === '--list') {
    process.stdout.write(`${STABLE_INSTALLER_NAMES.join('\n')}\n`);
    return;
  }
  if (args.length !== 2) {
    console.error('usage: stable-installer-name.mjs --list | <version> <versioned-file>');
    process.exit(2);
  }
  try {
    process.stdout.write(`${stableInstallerName(args[0], args[1])}\n`);
  } catch (err) {
    console.error(err.message);
    process.exit(1);
  }
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
