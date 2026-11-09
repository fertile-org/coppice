#!/usr/bin/env node
// Fails when the old update domain appears anywhere we ship. That domain was
// never bought, so a stale reference in a build points installs at a host we
// don't own. The pattern is built at runtime so this file doesn't match itself.
//
//   node desktop/scripts/check-old-domain.mjs --source [path ...]
//
// --source scans every path `git ls-files` lists. A tracked symlink is always
// checked by its link text. When that link resolves to a file inside this
// repository, the file is checked too. A link that resolves to a directory
// inside the repository is not read; those files are already listed under
// their own paths. A broken link, or a link that resolves outside the
// repository, fails the check. Each extra path (a file or a directory, such
// as the packaged app, updater-mode.json or the Pages CNAME) must exist and
// is read the same way as before. Anything that should be scanned but can't
// be read fails the check instead of passing.
import { spawnSync } from 'node:child_process';
import { lstatSync, readdirSync, readFileSync, readlinkSync, realpathSync, statSync } from 'node:fs';
import { isAbsolute, join, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

export const OLD_DOMAIN = ['getcoppice', 'dev'].join('.');
const PATTERN = new RegExp(OLD_DOMAIN.replaceAll('.', '[.]'), 'i');

// Matches as `file:line`, from raw bytes so app.asar and binaries are covered.
export function findInBuffer(buffer, label) {
  const text = buffer.toString('latin1');
  if (!PATTERN.test(text)) return [];
  const hits = [];
  text.split('\n').forEach((line, index) => {
    if (PATTERN.test(line)) hits.push(`${label}:${index + 1}`);
  });
  return hits;
}

function walk(path, out) {
  const stat = statSync(path);
  if (stat.isDirectory()) {
    for (const entry of readdirSync(path)) walk(join(path, entry), out);
  } else if (stat.isFile()) {
    out.push(path);
  }
}

export function sourceFiles(root) {
  const listed = spawnSync('git', ['ls-files', '-z'], { cwd: root, encoding: 'utf8' });
  if (listed.status !== 0) {
    throw new Error(`git ls-files failed: ${(listed.stderr || '').trim() || listed.error}`);
  }
  const files = listed.stdout.split('\0').filter(Boolean);
  if (!files.length) throw new Error('git ls-files listed no files');
  return files.map((file) => join(root, file));
}

// True when `candidateReal` is `rootReal` or a path under it. Both arguments
// must already be realpaths, so a symlink can't pretend to stay inside.
function pathIsInside(rootReal, candidateReal) {
  const rel = relative(rootReal, candidateReal);
  return rel === '' || (!isAbsolute(rel) && rel !== '..' && !rel.startsWith(`..${sep}`));
}

// Hits for one `git ls-files` entry. A deleted worktree file is skipped: it
// is not shipped. A symlink is not skipped that way. Its link text is
// scanned, a file it resolves to inside the repository is scanned, a
// directory inside the repository is left to its own tracked paths, and a
// broken or out-of-repo link fails the check.
function trackedEntryHits(file, root, rootReal) {
  let info;
  try {
    info = lstatSync(file);
  } catch (err) {
    if (err.code === 'ENOENT') return [];
    throw err;
  }
  const label = relative(root, file) || file;
  if (!info.isSymbolicLink()) return findInBuffer(readFileSync(file), label);

  const target = readlinkSync(file);
  const hits = findInBuffer(Buffer.from(target), label);
  let resolved;
  try {
    resolved = realpathSync(file);
  } catch (err) {
    if (err.code === 'ENOENT' || err.code === 'ENOTDIR' || err.code === 'ELOOP') {
      throw new Error(`broken symlink: ${label} -> ${target}`);
    }
    throw err;
  }
  if (!pathIsInside(rootReal, resolved)) {
    throw new Error(`symlink resolves outside the repository: ${label} -> ${target}`);
  }
  const resolvedStat = statSync(resolved);
  if (resolvedStat.isDirectory()) return hits;
  if (!resolvedStat.isFile()) {
    throw new Error(`symlink does not resolve to a file or directory: ${label} -> ${target}`);
  }
  const resolvedLabel = relative(rootReal, resolved) || resolved;
  hits.push(...findInBuffer(readFileSync(resolved), resolvedLabel));
  return hits;
}

export function checkOldDomain({ source = false, paths = [], root = process.cwd() } = {}) {
  if (!source && !paths.length) throw new Error('nothing to check: pass --source or a path');
  const files = [];
  const hits = [];
  if (source) {
    const rootReal = realpathSync(root);
    for (const file of sourceFiles(root)) {
      files.push(file);
      hits.push(...trackedEntryHits(file, root, rootReal));
    }
  }
  // Extra paths (the packaged app, updater-mode.json, the CNAME) stay on the
  // original read path. Symlink policy above applies only to tracked entries.
  const extras = [];
  for (const path of paths) walk(path, extras);
  for (const file of extras) {
    files.push(file);
    let buffer;
    try {
      buffer = readFileSync(file);
    } catch (err) {
      if (source && err.code === 'ENOENT' && !paths.includes(file)) continue;
      throw err;
    }
    hits.push(...findInBuffer(buffer, relative(root, file) || file));
  }
  return { files: files.length, hits };
}

function main(argv) {
  const source = argv.includes('--source');
  const paths = argv.filter((arg) => arg !== '--source');
  try {
    const { files, hits } = checkOldDomain({ source, paths });
    if (hits.length) {
      for (const hit of hits) console.error(hit);
      console.error(`${OLD_DOMAIN} found in ${hits.length} place(s). Use getcoppice.app.`);
      return 1;
    }
    console.log(`No ${OLD_DOMAIN} in ${files} file(s).`);
    return 0;
  } catch (err) {
    console.error(`Old domain check could not run: ${err.message}`);
    return 1;
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  process.exit(main(process.argv.slice(2)));
}
