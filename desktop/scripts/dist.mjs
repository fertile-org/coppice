#!/usr/bin/env node
// Runs electron-builder with signing and notarization switched on only when
// their secrets are present; otherwise the build is unsigned.
//
//   node scripts/dist.mjs [--dir]

import { spawn } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
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
