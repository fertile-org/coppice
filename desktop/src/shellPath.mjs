import { execFile } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const MARKER = '__COPPICE_PATH__';
const PRINT_PATH = `printf '${MARKER}%s${MARKER}' "$PATH"`;

function defaultRun(argv, timeoutMs) {
  const [command, ...args] = argv;
  return new Promise((resolve, reject) => {
    execFile(
      command,
      args,
      { timeout: timeoutMs, killSignal: 'SIGKILL', maxBuffer: 1024 * 1024, encoding: 'utf8' },
      (err, stdout) => (err ? reject(err) : resolve(stdout)),
    );
  });
}

function newestNvmBin(home) {
  const root = path.join(home, '.nvm', 'versions', 'node');
  let best = null;
  let bestKey = null;
  let names;
  try {
    names = fs.readdirSync(root);
  } catch {
    return null;
  }
  for (const name of names) {
    const key = versionKey(name);
    if (!key) continue;
    const bin = path.join(root, name, 'bin');
    if (!fs.existsSync(bin)) continue;
    if (!best || compareVersion(key, bestKey) > 0) {
      best = bin;
      bestKey = key;
    }
  }
  return best;
}

function versionKey(name) {
  const stripped = name.replace(/^[vV]/, '');
  const parts = [];
  for (const part of stripped.split('.')) {
    const digits = part.match(/^\d+/);
    if (!digits) break;
    parts.push(Number(digits[0]));
  }
  return parts.length ? parts : null;
}

function compareVersion(a, b) {
  const n = Math.max(a.length, b.length);
  for (let i = 0; i < n; i += 1) {
    const av = a[i] ?? 0;
    const bv = b[i] ?? 0;
    if (av !== bv) return av - bv;
  }
  return 0;
}

function existing(dir) {
  try {
    return fs.existsSync(dir) ? dir : null;
  } catch {
    return null;
  }
}

function fallbackPath(home) {
  const extras = [
    newestNvmBin(home),
    path.join(home, '.local', 'share', 'mise', 'shims'),
    path.join(home, '.asdf', 'shims'),
    path.join(home, '.volta', 'bin'),
    path.join(home, '.cargo', 'bin'),
    path.join(home, '.opencode', 'bin'),
    path.join(home, '.npm-global', 'bin'),
    path.join(home, '.bun', 'bin'),
  ]
    .map((dir) => (dir ? existing(dir) : null))
    .filter(Boolean);
  const entries = [
    ...(process.env.PATH ?? '').split(path.delimiter),
    ...extras,
    '/opt/homebrew/bin',
    '/usr/local/bin',
    path.join(home, '.local', 'bin'),
  ].filter(Boolean);
  return [...new Set(entries)].join(path.delimiter);
}

export async function resolveLoginShellPath({
  shell = process.env.SHELL,
  timeoutMs = 8000,
  run = defaultRun,
  home = os.homedir(),
} = {}) {
  if (!shell) return fallbackPath(home);
  let timer;
  try {
    const timeout = new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error('login shell timed out')), timeoutMs);
    });
    const output = await Promise.race([run([shell, '-ilc', PRINT_PATH], timeoutMs), timeout]);
    const start = output.indexOf(MARKER);
    const end = output.indexOf(MARKER, start + MARKER.length);
    if (start === -1 || end === -1) return fallbackPath(home);
    const value = output.slice(start + MARKER.length, end);
    return value || fallbackPath(home);
  } catch {
    return fallbackPath(home);
  } finally {
    clearTimeout(timer);
  }
}
