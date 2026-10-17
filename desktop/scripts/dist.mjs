#!/usr/bin/env node
// Runs electron-builder with signing and notarization switched on only when
// their secrets are present; otherwise the build is unsigned.
//
//   node scripts/dist.mjs [--dir]

import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { aptSourceLine, updateBase } from '../src/updateEndpoints.mjs';
import { installAptPublicKey } from './apt-repo.mjs';
import { stampDebsInDist } from './deb-version.mjs';
import { writeMetainfo } from './write-metainfo.mjs';

const desktopDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');

// GitHub Actions sets an absent secret to '', which electron-builder would treat
// as a real value (e.g. importCertificate('')), so blank ones are removed.
const SECRET_KEYS = new Set([
  'CSC_LINK',
  'CSC_KEY_PASSWORD',
  'APPLE_API_KEY',
  'APPLE_API_KEY_ID',
  'APPLE_API_ISSUER',
]);

// In-place macOS install is this one condition. A non-blank CSC_LINK means the
// build is Developer ID signed, so updater-mode.json tells the app to use
// electron-updater. Unsigned builds keep the download link. No other code
// change is required to turn in-place updates on.
export function macInPlaceUpdatesEnabled(env) {
  return typeof env.CSC_LINK === 'string' && env.CSC_LINK.trim() !== '';
}

export function updaterModeJson(env) {
  return `${JSON.stringify({
    macInPlaceUpdates: macInPlaceUpdatesEnabled(env),
    updateBase: updateBase(env.COPPICE_UPDATE_BASE),
  })}\n`;
}

// Writes the public keyring and the apt source line into resources/apt.
// The private signing key is never read here.
export function stageAptResources(env, resourcesDir) {
  const aptDir = join(resourcesDir, 'apt');
  const keyPath = join(aptDir, 'coppice-archive-keyring.gpg');
  const listPath = join(aptDir, 'coppice.list');
  const installed = installAptPublicKey({
    publicKeyArmored: env.COPPICE_APT_PUBLIC_KEY ?? '',
    outPath: keyPath,
  });
  if (!installed) {
    rmSync(listPath, { force: true });
    return false;
  }
  mkdirSync(aptDir, { recursive: true });
  writeFileSync(listPath, `${aptSourceLine(env.COPPICE_UPDATE_BASE)}\n`);
  return true;
}

export function builderArgs(env, { dir, platform }) {
  const outEnv = {};
  for (const [key, value] of Object.entries(env)) {
    if (value === undefined) continue;
    if (SECRET_KEYS.has(key) && value.trim() === '') continue;
    outEnv[key] = value;
  }

  const args = ['--publish=never'];
  if (dir) args.push('--dir');
  if (platform === 'darwin' && outEnv.APPLE_API_KEY && outEnv.CSC_LINK) {
    args.push('-c.mac.notarize=true');
  }
  if (!outEnv.CSC_LINK) outEnv.CSC_IDENTITY_AUTO_DISCOVERY = 'false';
  return { args, env: outEnv };
}

function main() {
  writeMetainfo(desktopDir);
  const { values } = parseArgs({ options: { dir: { type: 'boolean', default: false } } });
  const { args, env } = builderArgs(process.env, { dir: values.dir, platform: process.platform });
  writeFileSync(join(desktopDir, 'updater-mode.json'), updaterModeJson(env));
  const resourcesDir = join(desktopDir, 'resources');
  if (process.platform === 'linux' && existsSync(resourcesDir)) {
    stageAptResources(env, resourcesDir);
  }
  delete env.COPPICE_APT_PUBLIC_KEY;
  for (const key of Object.keys(env)) {
    if (key.startsWith('COPPICE_APT_SIGNING_')) delete env[key];
  }
  const bin = join(desktopDir, 'node_modules', '.bin', 'electron-builder');
  console.log(`electron-builder ${args.join(' ')}`);
  const child = spawn(bin, args, { cwd: desktopDir, env, stdio: 'inherit' });
  child.on('error', (err) => {
    console.error(`failed to start electron-builder: ${err.message}`);
    process.exit(1);
  });
  child.on('exit', (code, signal) => {
    if (code !== 0 || signal) process.exit(code ?? 1);
    if (values.dir || process.platform !== 'linux') process.exit(0);
    stampDebsInDist(desktopDir).then(
      () => process.exit(0),
      (err) => {
        console.error(err.message);
        process.exit(1);
      },
    );
  });
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
