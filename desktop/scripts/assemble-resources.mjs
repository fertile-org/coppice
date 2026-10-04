#!/usr/bin/env node
// Assembles the packaged resources directory (electron-builder extraResources):
//   <out>/{bin/coppice-server, postgres/{bin,lib,share}, web/, agent-templates/, fixtures/agent-responses/}
//
// On Linux the shared libraries the Postgres binaries link against are copied
// into postgres/lib (the server runs them with LD_LIBRARY_PATH=postgres/lib),
// except glibc and the toolchain runtime, which every target system provides.
// On macOS every Mach-O must link only system libraries or files in postgres/lib.
//
// --postgres must be a flat layout: bin/, lib/ (modules and shared libs side by
// side) and share/extension/, as in the pinned theseus bundles. pg-embed's nested
// layout (lib/postgresql/, share/postgresql/) is accepted for local testing only
// and is not pruned.
//
//   node scripts/assemble-resources.mjs --server-bin <path> --web-dist <path> --postgres <dir> [--out <dir>]

import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
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
  /^libmvec\.so/,
  /^libanl\.so/,
  /^libnsl\.so\.1$/,
  /^libBrokenLocale\.so/,
];

const BUILD_ONLY_DIRS = ['include', 'lib/pgxs', 'lib/pkgconfig'];
const BUILD_ONLY_LIB = [/\.a$/];

// Linux only: PL/Python needs a full Python runtime and uuid-ossp needs libossp-uuid;
// Coppice uses neither, so they are left out rather than shipped broken.
const LINUX_DROPPED_LIB = [/plpython3\.so$/, /^uuid-ossp\.so$/];
const LINUX_DROPPED_EXTENSION = [/plpython3u/, /^uuid-ossp[.-]/];

const MACHO_MAGICS = new Set([0xfeedface, 0xfeedfacf, 0xcefaedfe, 0xcffaedfe, 0xcafebabe]);
const MACHO_SYSTEM_PREFIXES = ['/usr/lib/', '/System/Library/'];

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

export function parseOtoolL(output) {
  const deps = [];
  for (const line of output.split('\n')) {
    if (!/^\s/.test(line)) continue;
    const dep = line.trim().replace(/ \(compatibility version [^)]*\)$/, '');
    if (dep && !deps.includes(dep)) deps.push(dep);
  }
  return deps;
}

export function parseOtoolLoadCommands(output) {
  let id = null;
  const rpaths = [];
  let cmd = null;
  for (const raw of output.split('\n')) {
    const line = raw.trim();
    const cmdMatch = line.match(/^cmd (\S+)$/);
    if (cmdMatch) {
      cmd = cmdMatch[1];
      continue;
    }
    const value = line.match(/^(name|path) (.+) \(offset \d+\)$/);
    if (!value) continue;
    if (cmd === 'LC_ID_DYLIB' && value[1] === 'name') id = value[2];
    else if (cmd === 'LC_RPATH' && value[1] === 'path' && !rpaths.includes(value[2])) rpaths.push(value[2]);
  }
  return { id, rpaths };
}

/** Dependencies of `file` that resolve neither to the OS nor to a file inside postgres/lib. */
export function machoDepProblems({ file, deps, id, rpaths, pgDir, exists }) {
  const libDir = join(pgDir, 'lib');
  const expand = (ref) =>
    ref.replace(/^@loader_path(?=\/|$)/, dirname(file)).replace(/^@executable_path(?=\/|$)/, join(pgDir, 'bin'));
  const bundled = (candidate) => {
    const abs = resolve(candidate);
    const rel = relative(libDir, abs);
    return rel !== '' && !rel.startsWith('..') && !rel.startsWith('/') && exists(abs);
  };

  const problems = [];
  for (const dep of deps) {
    if (dep === id) continue;
    if (MACHO_SYSTEM_PREFIXES.some((prefix) => dep.startsWith(prefix))) continue;
    let ok = false;
    if (dep.startsWith('@rpath/')) {
      const rest = dep.slice('@rpath/'.length);
      const candidates =
        rpaths.length > 0 ? rpaths.map((r) => join(expand(r), rest)) : [join(libDir, rest.split('/').pop())];
      ok = candidates.some(bundled);
    } else if (dep.startsWith('@loader_path/') || dep.startsWith('@executable_path/')) {
      ok = bundled(expand(dep));
    }
    if (!ok) problems.push({ file, dep });
  }
  return problems;
}

export function unresolvedMachoMessage(problems) {
  const lines = problems.map(({ file, dep }) => `  ${file} -> ${dep}`);
  return `bundled postgres links libraries outside the bundle (only /usr/lib, /System/Library and postgres/lib are allowed):\n${lines.join('\n')}`;
}

async function readMagic(path) {
  const handle = await open(path, 'r');
  try {
    const { buffer, bytesRead } = await handle.read(Buffer.alloc(4), 0, 4, 0);
    return bytesRead === 4 ? buffer : null;
  } finally {
    await handle.close();
  }
}

async function isElf(path) {
  const magic = await readMagic(path);
  return magic !== null && magic.toString('latin1') === '\x7fELF';
}

async function isMachO(path) {
  const magic = await readMagic(path);
  return magic !== null && MACHO_MAGICS.has(magic.readUInt32BE(0));
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

function runOtool(flag, file) {
  const res = spawnSync('otool', [flag, file], { encoding: 'utf8' });
  if (res.error) throw new Error(`otool ${flag} ${file}: ${res.error.message}`);
  if (res.status !== 0) throw new Error(`otool ${flag} ${file} failed: ${res.stderr.trim()}`);
  return res.stdout;
}

async function machOFiles(pgDir) {
  const files = [];
  for (const [sub, recursive] of [
    ['bin', false],
    ['lib', true],
  ]) {
    for (const name of await readdir(join(pgDir, sub), { recursive })) {
      const path = join(pgDir, sub, name);
      if ((await lstat(path)).isFile() && (await isMachO(path))) files.push(path);
    }
  }
  return files;
}

async function verifyMachOClosure(pgDir) {
  const problems = [];
  for (const file of await machOFiles(pgDir)) {
    const { id, rpaths } = parseOtoolLoadCommands(runOtool('-l', file));
    const deps = parseOtoolL(runOtool('-L', file));
    for (const p of machoDepProblems({ file, deps, id, rpaths, pgDir, exists: existsSync })) {
      problems.push({ file: relative(pgDir, p.file), dep: p.dep });
    }
  }
  if (problems.length > 0) throw new Error(unresolvedMachoMessage(problems));
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
  platform = process.platform,
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
  for (const dir of BUILD_ONLY_DIRS) await rm(join(pgOut, dir), { recursive: true, force: true });
  await pruneEntries(join(pgOut, 'lib'), BUILD_ONLY_LIB);
  if (platform === 'linux') {
    await pruneEntries(join(pgOut, 'lib'), LINUX_DROPPED_LIB);
    await pruneEntries(join(pgOut, 'share', 'extension'), LINUX_DROPPED_EXTENSION);
    await bundleSharedLibs(pgOut);
  } else if (platform === 'darwin') {
    await verifyMachOClosure(pgOut);
  }

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
