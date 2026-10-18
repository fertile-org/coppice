#!/usr/bin/env node
// Fails when the old update domain appears anywhere we ship. That domain was
// never bought, so a stale reference in a build points installs at a host we
// don't own. The pattern is built at runtime so this file doesn't match itself.
//
//   node desktop/scripts/check-old-domain.mjs --source [path ...]
//
// --source scans every file `git ls-files` lists. Each extra path (a file or a
// directory, such as the packaged app, updater-mode.json or the Pages CNAME)
// must exist. Anything that can't be read fails the check instead of passing.
import { spawnSync } from 'node:child_process';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
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

export function checkOldDomain({ source = false, paths = [], root = process.cwd() } = {}) {
  if (!source && !paths.length) throw new Error('nothing to check: pass --source or a path');
  const files = source ? sourceFiles(root) : [];
  for (const path of paths) walk(path, files);
  const hits = [];
  for (const file of files) {
    let buffer;
    try {
      buffer = readFileSync(file);
    } catch (err) {
      // A tracked file deleted in the working tree is not shipped.
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
