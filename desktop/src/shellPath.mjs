import { execFile } from 'node:child_process';
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

function fallbackPath() {
  const entries = [
    ...(process.env.PATH ?? '').split(path.delimiter),
    '/opt/homebrew/bin',
    '/usr/local/bin',
    path.join(os.homedir(), '.local', 'bin'),
  ].filter(Boolean);
  return [...new Set(entries)].join(path.delimiter);
}

export async function resolveLoginShellPath({
  shell = process.env.SHELL,
  timeoutMs = 5000,
  run = defaultRun,
} = {}) {
  if (!shell) return fallbackPath();
  let timer;
  try {
    const timeout = new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error('login shell timed out')), timeoutMs);
    });
    const output = await Promise.race([run([shell, '-ilc', PRINT_PATH], timeoutMs), timeout]);
    const start = output.indexOf(MARKER);
    const end = output.indexOf(MARKER, start + MARKER.length);
    if (start === -1 || end === -1) return fallbackPath();
    const value = output.slice(start + MARKER.length, end);
    return value || fallbackPath();
  } catch {
    return fallbackPath();
  } finally {
    clearTimeout(timer);
  }
}
