#!/usr/bin/env node
// Downloads the pinned Postgres bundle for a target triple, verifies its
// SHA-256 against postgres.lock.json and extracts it to <out>/{bin,lib,share}.
//
//   node scripts/fetch-postgres.mjs [--target <triple>] --out <dir>

import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { access, mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

const desktopDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const lockPath = join(desktopDir, 'postgres.lock.json');
const cacheDir = join(desktopDir, '.cache', 'postgres');

const HOST_TARGETS = {
  'darwin-arm64': 'aarch64-apple-darwin',
  'darwin-x64': 'x86_64-apple-darwin',
  'linux-x64': 'x86_64-unknown-linux-gnu',
  'linux-arm64': 'aarch64-unknown-linux-gnu',
};

export function targetForHost(platform, arch) {
  const target = HOST_TARGETS[`${platform}-${arch}`];
  if (!target) {
    throw new Error(`unsupported host for bundled postgres: ${platform}/${arch}`);
  }
  return target;
}

async function sha256File(file) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) {
    hash.update(chunk);
  }
  return hash.digest('hex');
}

export async function verifySha256(file, expected) {
  const actual = await sha256File(file);
  if (actual !== expected.toLowerCase()) {
    throw new Error(`checksum mismatch for ${file}: expected ${expected}, got ${actual}`);
  }
}

async function exists(path) {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

async function download(url, dest) {
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`download failed for ${url}: HTTP ${res.status}`);
  }
  await mkdir(dirname(dest), { recursive: true });
  const partial = `${dest}.partial`;
  await writeFile(partial, Buffer.from(await res.arrayBuffer()));
  await rename(partial, dest);
}

export async function fetchPostgres(target, outDir) {
  const lock = JSON.parse(await readFile(lockPath, 'utf8'));
  const entry = lock.targets[target];
  if (!entry) {
    throw new Error(`no postgres bundle pinned for target ${target}`);
  }

  const archive = join(cacheDir, basename(new URL(entry.url).pathname));
  if (!(await exists(archive))) {
    console.log(`downloading ${entry.url}`);
    await download(entry.url, archive);
  }
  try {
    await verifySha256(archive, entry.sha256);
  } catch (err) {
    await rm(archive, { force: true });
    throw err;
  }

  await rm(outDir, { recursive: true, force: true });
  await mkdir(outDir, { recursive: true });
  execFileSync('tar', ['-xzf', archive, '-C', outDir, '--strip-components=1'], {
    stdio: 'inherit',
  });
  console.log(`postgres ${lock.version} (${target}) extracted to ${outDir}`);
}

async function main() {
  const { values } = parseArgs({
    options: {
      target: { type: 'string' },
      out: { type: 'string' },
    },
  });
  if (!values.out) {
    throw new Error('usage: fetch-postgres.mjs [--target <triple>] --out <dir>');
  }
  const target = values.target ?? targetForHost(process.platform, process.arch);
  await fetchPostgres(target, resolve(values.out));
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((err) => {
    console.error(err.message);
    process.exit(1);
  });
}
