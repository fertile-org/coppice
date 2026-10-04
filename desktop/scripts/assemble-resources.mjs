#!/usr/bin/env node
// Assembles the packaged resources directory (electron-builder extraResources):
//   <out>/{bin/coppice-server, postgres/{bin,lib,share}, web/, agent-templates/, fixtures/agent-responses/}
//
// On Linux the shared libraries the Postgres binaries link against are copied
// into postgres/lib (the server runs them with LD_LIBRARY_PATH=postgres/lib),
// except glibc and the toolchain runtime, which every target system provides.
//
//   node scripts/assemble-resources.mjs --server-bin <path> --web-dist <path> --postgres <dir> [--out <dir>]

import { spawnSync } from 'node:child_process';
import { chmod, copyFile, cp, lstat, mkdir, open, readdir, rm } from 'node:fs/promises';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

const desktopDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');

// Provided by every glibc system; bundling them breaks the loader/libc pairing.
const BASE_LIBS = [
  /^linux-vdso\.so/,
  /^ld-linux.*\.so/,
  /^libc\.so/,
  /^libm\.so/,
  /^libpthread\.so/,
  /^libdl\.so/,
  /^librt\.so/,
  /^libutil\.so/,
  /^libresolv\.so/,
  /^libgcc_s\.so/,
  /^libstdc\+\+\.so/,
];

// PL/Python needs a full Python runtime and uuid-ossp needs libossp-uuid; Coppice
// uses neither, so they are left out rather than shipped broken.
const DROPPED_LIB = [/plpython3\.so$/, /^uuid-ossp\.so$/, /\.a$/];
const DROPPED_EXTENSION = [/plpython3u/, /^uuid-ossp[.-]/];
const DROPPED_DIRS = ['include', 'lib/pgxs', 'lib/pkgconfig'];

export function isBaseLib(name) {
  return BASE_LIBS.some((re) => re.test(name));
}

export function parseLdd(output) {
  const resolved = [];
  const missing = [];
  for (const raw of output.split('\n')) {
    const line = raw.trim();
    const arrow = line.match(/^(\S+) => (.+?)(?: \(0x[0-9a-f]+\))?$/);
    if (!arrow) continue;
    const [, name, target] = arrow;
    if (target === 'not found') missing.push(name);
    else if (target.startsWith('/')) resolved.push({ name, path: target });
  }
  return { resolved, missing };
}

export function planLibCopies(results, present) {
  const copy = [];
  const missing = [];
  const planned = new Set();
  for (const { file, ldd } of results) {
    for (const name of ldd.missing) {
      if (!present.has(name)) missing.push({ file, name });
    }
    for (const lib of ldd.resolved) {
      if (isBaseLib(lib.name) || present.has(lib.name) || planned.has(lib.name)) continue;
      planned.add(lib.name);
      copy.push(lib);
    }
  }
  return { copy, missing };
}

export function missingLibsMessage(missing) {
  const lines = missing.map(({ file, name }) => `  ${name} (needed by ${file})`);
  return `shared libraries not found; install them on the build host so they can be bundled:\n${lines.join('\n')}`;
}

async function isElf(path) {
  const handle = await open(path, 'r');
  try {
    const { buffer, bytesRead } = await handle.read(Buffer.alloc(4), 0, 4, 0);
    return bytesRead === 4 && buffer.toString('latin1') === '\x7fELF';
  } finally {
    await handle.close();
  }
}

async function elfFiles(pgDir) {
  const files = [];
  for (const [sub, match] of [
    ['bin', () => true],
    ['lib', (name) => /\.so(\.|$)/.test(name)],
  ]) {
    for (const name of await readdir(join(pgDir, sub))) {
      const path = join(pgDir, sub, name);
      const st = await lstat(path);
      if (st.isFile() && match(name) && (await isElf(path))) files.push(path);
    }
  }
  return files;
}

function runLdd(file, libDir) {
  const hostPath = process.env.LD_LIBRARY_PATH;
  const res = spawnSync('ldd', [file], {
    encoding: 'utf8',
    env: { ...process.env, LD_LIBRARY_PATH: hostPath ? `${libDir}:${hostPath}` : libDir },
  });
  if (res.error) throw new Error(`ldd ${file}: ${res.error.message}`);
  return res.stdout;
}

async function bundleSharedLibs(pgDir) {
  const libDir = join(pgDir, 'lib');
  const checked = new Set();
  for (;;) {
    const files = (await elfFiles(pgDir)).filter((f) => !checked.has(f));
    if (files.length === 0) return;
    const results = files.map((file) => {
      checked.add(file);
      return { file: relative(pgDir, file), ldd: parseLdd(runLdd(file, libDir)) };
    });
    const plan = planLibCopies(results, new Set(await readdir(libDir)));
    if (plan.missing.length > 0) throw new Error(missingLibsMessage(plan.missing));
    if (plan.copy.length === 0) return;
    for (const lib of plan.copy) {
      console.log(`bundling ${lib.name} from ${lib.path}`);
      await copyFile(lib.path, join(libDir, lib.name));
    }
  }
}

async function pruneEntries(dir, patterns) {
  let names;
  try {
    names = await readdir(dir);
  } catch (err) {
    if (err.code === 'ENOENT') return;
    throw err;
  }
  for (const name of names) {
    if (patterns.some((re) => re.test(name))) await rm(join(dir, name), { recursive: true, force: true });
  }
}

export async function assembleResources({
  serverBin,
  webDist,
  postgres,
  out,
  repoRoot = resolve(desktopDir, '..'),
  bundleLibs = process.platform === 'linux',
}) {
  await rm(out, { recursive: true, force: true });
  await mkdir(join(out, 'bin'), { recursive: true });

  const server = join(out, 'bin', 'coppice-server');
  await copyFile(serverBin, server);
  await chmod(server, 0o755);

  const pgOut = join(out, 'postgres');
  for (const sub of ['bin', 'lib', 'share']) {
    await cp(join(postgres, sub), join(pgOut, sub), { recursive: true, verbatimSymlinks: true });
  }
  for (const dir of DROPPED_DIRS) await rm(join(pgOut, dir), { recursive: true, force: true });
  await pruneEntries(join(pgOut, 'lib'), DROPPED_LIB);
  await pruneEntries(join(pgOut, 'share', 'extension'), DROPPED_EXTENSION);
  if (bundleLibs) await bundleSharedLibs(pgOut);

  await cp(webDist, join(out, 'web'), { recursive: true });
  await cp(join(repoRoot, 'server', 'agent_templates'), join(out, 'agent-templates'), { recursive: true });
  await cp(join(repoRoot, 'fixtures', 'agent-responses'), join(out, 'fixtures', 'agent-responses'), {
    recursive: true,
  });
}

async function main() {
  const { values } = parseArgs({
    options: {
      'server-bin': { type: 'string' },
      'web-dist': { type: 'string' },
      postgres: { type: 'string' },
      out: { type: 'string', default: join(desktopDir, 'resources') },
    },
  });
  if (!values['server-bin'] || !values['web-dist'] || !values.postgres) {
    throw new Error(
      'usage: assemble-resources.mjs --server-bin <path> --web-dist <path> --postgres <dir> [--out <dir>]',
    );
  }
  const out = resolve(values.out);
  await assembleResources({
    serverBin: resolve(values['server-bin']),
    webDist: resolve(values['web-dist']),
    postgres: resolve(values.postgres),
    out,
  });
  console.log(`resources assembled in ${out}`);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((err) => {
    console.error(err.message);
    process.exit(1);
  });
}
