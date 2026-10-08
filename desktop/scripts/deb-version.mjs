#!/usr/bin/env node
// Stamps or checks the Debian control Version of a built .deb.
//
// electron-builder names the file from package.json (`Coppice-0.1.0-rc.N-…`)
// and passes the deb control Version with "-" rewritten to "~". That is the
// version apt must see: 0.1.0~rc.N sorts before 0.1.0. This script enforces
// the same mapping on the control file only. File names, the app version, and
// macOS packages stay as they are. A matching Version is left unchanged.
//
//   node scripts/deb-version.mjs --check|--stamp <file.deb>...
//
// The app version is read from desktop/package.json (release-version.mjs
// writes the tag version there before packaging).

import { execFile } from 'node:child_process';
import { mkdtemp, readdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { promisify } from 'node:util';

import { debianPackageVersion } from './release-version.mjs';

const execFileAsync = promisify(execFile);
const desktopDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');

export async function readDebVersion(debPath) {
  const { stdout } = await execFileAsync('dpkg-deb', ['-f', debPath, 'Version']);
  return stdout.trim();
}

export function replaceControlVersion(control, version) {
  if (!/^Version: /m.test(control)) {
    throw new Error('control file has no Version field');
  }
  const next = control.replace(/^Version:.*$/m, `Version: ${version}`);
  return next.endsWith('\n') ? next : `${next}\n`;
}

export async function stampDebVersion(debPath, appVersion) {
  const expected = debianPackageVersion(appVersion);
  const actual = await readDebVersion(debPath);
  if (actual === expected) return { version: expected, rewritten: false };

  const dir = await mkdtemp(join(tmpdir(), 'deb-stamp-'));
  const staged = `${debPath}.stamped`;
  try {
    await execFileAsync('dpkg-deb', ['-R', debPath, dir]);
    const controlPath = join(dir, 'DEBIAN', 'control');
    const control = await readFile(controlPath, 'utf8');
    await writeFile(controlPath, replaceControlVersion(control, expected));
    await execFileAsync('dpkg-deb', ['--root-owner-group', '--build', dir, staged]);
    await rename(staged, debPath);
  } finally {
    await rm(dir, { recursive: true, force: true });
    await rm(staged, { force: true });
  }

  const version = await readDebVersion(debPath);
  if (version !== expected) {
    throw new Error(`${debPath}: Version is ${version}, expected ${expected}`);
  }
  return { version, rewritten: true };
}

function assertFileName(file, appVersion) {
  const name = basename(file);
  const marker = `Coppice-${appVersion}-linux-`;
  if (!name.startsWith(marker) || !name.endsWith('.deb') || name.includes('~')) {
    throw new Error(`${name}: file name must keep the app version ${appVersion}, not the Debian version`);
  }
}

export async function stampDebsInDist(dir) {
  const pkg = JSON.parse(await readFile(join(dir, 'package.json'), 'utf8'));
  const dist = join(dir, 'dist');
  const names = (await readdir(dist)).filter((name) => name.endsWith('.deb'));
  if (names.length === 0) throw new Error(`no .deb in ${dist}`);
  for (const name of names) {
    const file = join(dist, name);
    assertFileName(file, pkg.version);
    const result = await stampDebVersion(file, pkg.version);
    console.log(`deb ${name} Version=${result.version}${result.rewritten ? ' (stamped)' : ''}`);
  }
}

async function main() {
  const args = process.argv.slice(2);
  const mode = args[0];
  const files = args.slice(1);
  if ((mode !== '--check' && mode !== '--stamp') || files.length === 0) {
    console.error('usage: deb-version.mjs --check|--stamp <file.deb>...');
    process.exit(2);
  }
  const pkg = JSON.parse(await readFile(join(desktopDir, 'package.json'), 'utf8'));
  for (const file of files) {
    assertFileName(file, pkg.version);
    if (mode === '--stamp') await stampDebVersion(file, pkg.version);
    const version = await readDebVersion(file);
    const expected = debianPackageVersion(pkg.version);
    if (version !== expected) {
      console.error(`${file}: Version ${version}, expected ${expected}`);
      process.exit(1);
    }
    console.log(`${basename(file)} Version=${version}`);
  }
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
