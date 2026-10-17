#!/usr/bin/env node
// Builds a signed apt repository from .deb files.
//
//   node scripts/apt-repo.mjs --deb a.deb --deb b.deb --out site --key key.asc [--passphrase-file pass]
//   node scripts/apt-repo.mjs export-pubkey --key key.asc --out keyring.gpg [--passphrase-file pass]
//
// The private key never goes into the repository. export-pubkey writes the
// binary public key apt's signed-by option expects.

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { gzipSync } from 'node:zlib';
import {
  APT_COMPONENT,
  APT_KEYRING_PATH,
  APT_SOURCE_LINE,
  APT_SUITE,
  PAGES_BASE,
} from '../src/updateEndpoints.mjs';

export { APT_KEYRING_PATH, APT_SOURCE_LINE, PAGES_BASE };

const APT_ORIGIN = 'Coppice';
const APT_LABEL = 'Coppice';

export function poolFilename(packageName, version, arch) {
  const letter = packageName.slice(0, 1) || 'c';
  return `pool/${APT_COMPONENT}/${letter}/${packageName}/${packageName}_${version}_${arch}.deb`;
}

export function renderPackageEntry(control, file) {
  const lines = [
    `Package: ${control.Package}`,
    `Version: ${control.Version}`,
    `Architecture: ${control.Architecture}`,
    `Maintainer: ${control.Maintainer}`,
    `Filename: ${file.filename}`,
    `Size: ${file.size}`,
    `MD5sum: ${file.md5}`,
    `SHA1: ${file.sha1}`,
    `SHA256: ${file.sha256}`,
  ];
  const description = String(control.Description ?? '').replace(/\r/g, '');
  if (description) {
    const [first, ...rest] = description.split('\n');
    lines.push(`Description: ${first}`);
    for (const line of rest) lines.push(line.trim() === '' ? ' .' : ` ${line}`);
  }
  return `${lines.join('\n')}\n`;
}

export function rfc1123(date) {
  const days = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
  const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  const pad = (n) => String(n).padStart(2, '0');
  return `${days[date.getUTCDay()]}, ${pad(date.getUTCDate())} ${months[date.getUTCMonth()]} ${date.getUTCFullYear()} ${pad(date.getUTCHours())}:${pad(date.getUTCMinutes())}:${pad(date.getUTCSeconds())} +0000`;
}

export function renderRelease({ date, architectures, files }) {
  const lines = [
    `Origin: ${APT_ORIGIN}`,
    `Label: ${APT_LABEL}`,
    `Suite: ${APT_SUITE}`,
    `Codename: ${APT_SUITE}`,
    `Architectures: ${architectures.join(' ')}`,
    `Components: ${APT_COMPONENT}`,
    `Date: ${date}`,
  ];
  for (const [header, key] of [
    ['MD5Sum', 'md5'],
    ['SHA1', 'sha1'],
    ['SHA256', 'sha256'],
  ]) {
    lines.push(`${header}:`);
    for (const file of files) lines.push(` ${file[key]} ${file.size} ${file.path}`);
  }
  return `${lines.join('\n')}\n`;
}

export function parseDebFieldOutput(text) {
  const fields = {};
  let current = null;
  for (const line of String(text).split('\n')) {
    if ((line.startsWith(' ') || line.startsWith('\t')) && current) {
      fields[current] = `${fields[current]}\n${line.slice(1)}`;
      continue;
    }
    const idx = line.indexOf(':');
    if (idx === -1) continue;
    current = line.slice(0, idx);
    fields[current] = line.slice(idx + 1).trim();
  }
  return fields;
}

export function readDebControl(debPath) {
  const result = spawnSync(
    'dpkg-deb',
    ['-f', debPath, 'Package', 'Version', 'Architecture', 'Maintainer', 'Description'],
    { encoding: 'utf8' },
  );
  if (result.status !== 0) {
    throw new Error((result.stderr || `dpkg-deb failed for ${debPath}`).trim());
  }
  return parseDebFieldOutput(result.stdout);
}

function digests(buffer) {
  return {
    md5: createHash('md5').update(buffer).digest('hex'),
    sha1: createHash('sha1').update(buffer).digest('hex'),
    sha256: createHash('sha256').update(buffer).digest('hex'),
    size: buffer.length,
  };
}

function gpg(gpgHome, args, { input } = {}) {
  const result = spawnSync(
    'gpg',
    ['--batch', '--yes', '--pinentry-mode', 'loopback', '--homedir', gpgHome, ...args],
    { input, encoding: 'buffer' },
  );
  if (result.status !== 0) {
    throw new Error((result.stderr || Buffer.from('gpg failed')).toString().trim());
  }
  return result.stdout;
}

function withKeyring(keyArmored, passphrase, fn) {
  const gpgHome = mkdtempSync(join(tmpdir(), 'coppice-apt-'));
  chmodSync(gpgHome, 0o700);
  try {
    const args = ['--import'];
    if (passphrase) {
      const passFile = join(gpgHome, 'pass');
      writeFileSync(passFile, passphrase, { mode: 0o600 });
      args.unshift('--passphrase-file', passFile);
    }
    gpg(gpgHome, args, { input: Buffer.from(keyArmored) });
    const listed = gpg(gpgHome, ['--with-colons', '--list-secret-keys']).toString();
    const fingerprint = listed
      .split('\n')
      .find((line) => line.startsWith('fpr:'))
      ?.split(':')[9];
    if (!fingerprint) throw new Error('signing key has no fingerprint');
    return fn({ gpgHome, fingerprint });
  } finally {
    rmSync(gpgHome, { recursive: true, force: true });
  }
}

function signRelease({ gpgHome, fingerprint, passphrase, releasePath, distDir }) {
  const passArgs = [];
  if (passphrase) {
    const passFile = join(gpgHome, 'pass');
    if (!existsSync(passFile)) writeFileSync(passFile, passphrase, { mode: 0o600 });
    passArgs.push('--passphrase-file', passFile);
  }
  gpg(gpgHome, [
    ...passArgs,
    '--local-user',
    fingerprint,
    '--detach-sign',
    '--output',
    join(distDir, 'Release.gpg'),
    releasePath,
  ]);
  gpg(gpgHome, [
    ...passArgs,
    '--local-user',
    fingerprint,
    '--clearsign',
    '--output',
    join(distDir, 'InRelease'),
    releasePath,
  ]);
}

export function buildAptRepo({ debPaths, outDir, keyArmored, passphrase = '', date = new Date() }) {
  if (!debPaths?.length) throw new Error('at least one .deb is required');
  return withKeyring(keyArmored, passphrase, ({ gpgHome, fingerprint }) => {
    const byArch = new Map();
    for (const debPath of debPaths) {
      const control = readDebControl(debPath);
      for (const field of ['Package', 'Version', 'Architecture', 'Maintainer']) {
        if (!control[field]) throw new Error(`${debPath} is missing ${field}`);
      }
      const bytes = readFileSync(debPath);
      const filename = poolFilename(control.Package, control.Version, control.Architecture);
      const dest = join(outDir, filename);
      mkdirSync(dirname(dest), { recursive: true });
      copyFileSync(debPath, dest);
      const entry = renderPackageEntry(control, { filename, ...digests(bytes) });
      const list = byArch.get(control.Architecture) ?? [];
      list.push(entry);
      byArch.set(control.Architecture, list);
    }

    const architectures = [...byArch.keys()].sort();
    const releaseFiles = [];
    for (const arch of architectures) {
      const packages = byArch.get(arch).join('');
      const rel = `${APT_COMPONENT}/binary-${arch}`;
      const dir = join(outDir, 'dists', APT_SUITE, rel);
      mkdirSync(dir, { recursive: true });
      const pkgBuf = Buffer.from(packages);
      const gzBuf = gzipSync(pkgBuf);
      writeFileSync(join(dir, 'Packages'), pkgBuf);
      writeFileSync(join(dir, 'Packages.gz'), gzBuf);
      for (const [name, buf] of [
        ['Packages', pkgBuf],
        ['Packages.gz', gzBuf],
      ]) {
        releaseFiles.push({ path: `${rel}/${name}`, ...digests(buf) });
      }
    }

    const distDir = join(outDir, 'dists', APT_SUITE);
    const releasePath = join(distDir, 'Release');
    writeFileSync(releasePath, renderRelease({ date: rfc1123(date), architectures, files: releaseFiles }));
    signRelease({ gpgHome, fingerprint, passphrase, releasePath, distDir });
    writeFileSync(join(outDir, '.nojekyll'), '');
    return { architectures };
  });
}

export function exportPublicKeyring({ keyArmored, passphrase = '', outPath }) {
  return withKeyring(keyArmored, passphrase, ({ gpgHome, fingerprint }) => {
    const binary = gpg(gpgHome, ['--export', fingerprint]);
    mkdirSync(dirname(outPath), { recursive: true });
    writeFileSync(outPath, binary);
  });
}

// Linux packaging calls this with the public key only. An empty value leaves
// the deb without an apt source; the postinst skips a missing key. A stale
// key from an earlier local build is removed so it cannot be shipped.
export function installAptPublicKey({ publicKeyArmored, outPath }) {
  if (typeof publicKeyArmored !== 'string' || publicKeyArmored.trim() === '') {
    rmSync(outPath, { force: true });
    return false;
  }
  const armored = publicKeyArmored.replace(/\r\n/g, '\n').trim();
  if (armored.includes('PRIVATE KEY BLOCK')) {
    throw new Error('COPPICE_APT_PUBLIC_KEY must be a public key, not a private key');
  }
  if (!armored.includes('BEGIN PGP PUBLIC KEY BLOCK')) {
    throw new Error('COPPICE_APT_PUBLIC_KEY must be an ASCII-armored PGP public key');
  }
  mkdirSync(dirname(outPath), { recursive: true });
  const dir = mkdtempSync(join(tmpdir(), 'coppice-apt-pub-'));
  try {
    const asc = join(dir, 'key.asc');
    writeFileSync(asc, `${armored}\n`);
    const result = spawnSync('gpg', ['--batch', '--yes', '--dearmor', '--output', outPath, asc], {
      encoding: 'utf8',
    });
    if (result.status !== 0) {
      throw new Error((result.stderr || 'gpg --dearmor failed').trim());
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
  return true;
}

function readPassphrase(file) {
  if (!file) return '';
  return readFileSync(file, 'utf8');
}

function main() {
  const argv = process.argv.slice(2);
  if (argv[0] === 'export-pubkey') {
    const { values } = parseArgs({
      args: argv.slice(1),
      options: {
        key: { type: 'string' },
        out: { type: 'string' },
        'passphrase-file': { type: 'string' },
      },
    });
    if (!values.key || !values.out) {
      console.error('usage: apt-repo.mjs export-pubkey --key key.asc --out keyring.gpg');
      process.exit(2);
    }
    exportPublicKeyring({
      keyArmored: readFileSync(values.key, 'utf8'),
      passphrase: readPassphrase(values['passphrase-file']),
      outPath: values.out,
    });
    return;
  }

  const { values } = parseArgs({
    args: argv,
    options: {
      deb: { type: 'string', multiple: true },
      out: { type: 'string' },
      key: { type: 'string' },
      'passphrase-file': { type: 'string' },
      date: { type: 'string' },
    },
  });
  if (!values.deb?.length || !values.out || !values.key) {
    console.error('usage: apt-repo.mjs --deb a.deb --deb b.deb --out site --key key.asc');
    process.exit(2);
  }
  buildAptRepo({
    debPaths: values.deb,
    outDir: values.out,
    keyArmored: readFileSync(values.key, 'utf8'),
    passphrase: readPassphrase(values['passphrase-file']),
    date: values.date ? new Date(values.date) : new Date(),
  });
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main();
  } catch (err) {
    console.error(err.message);
    process.exit(1);
  }
}
