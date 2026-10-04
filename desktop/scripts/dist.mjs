#!/usr/bin/env node
// Runs electron-builder with signing and notarization switched on only when
// their secrets are present; otherwise the build is unsigned.
//
//   node scripts/dist.mjs [--dir]

import { spawn } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

const desktopDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');

export function builderArgs(env, { dir, platform }) {
  const args = ['--publish=never'];
  if (dir) args.push('--dir');
  if (platform === 'darwin' && env.APPLE_API_KEY) args.push('-c.mac.notarize=true');

  const outEnv = {};
  for (const [key, value] of Object.entries(env)) {
    if (value !== undefined) outEnv[key] = value;
  }
  if (!env.CSC_LINK) outEnv.CSC_IDENTITY_AUTO_DISCOVERY = 'false';
  return { args, env: outEnv };
}

function main() {
  const { values } = parseArgs({ options: { dir: { type: 'boolean', default: false } } });
  const { args, env } = builderArgs(process.env, { dir: values.dir, platform: process.platform });
  const bin = join(desktopDir, 'node_modules', '.bin', 'electron-builder');
  console.log(`electron-builder ${args.join(' ')}`);
  const child = spawn(bin, args, { cwd: desktopDir, env, stdio: 'inherit' });
  child.on('error', (err) => {
    console.error(`failed to start electron-builder: ${err.message}`);
    process.exit(1);
  });
  child.on('exit', (code, signal) => process.exit(code ?? (signal ? 1 : 0)));
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
