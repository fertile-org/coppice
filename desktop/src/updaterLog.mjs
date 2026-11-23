import fs from 'node:fs';
import path from 'node:path';

const UPDATER_LOG = 'updater.log';

/** Append one line to `updater.log` under the app logs directory. */
export function appendUpdaterLog(logsDir, message, { appendFileSync = fs.appendFileSync, mkdirSync = fs.mkdirSync } = {}) {
  if (typeof logsDir !== 'string' || logsDir.length === 0) return;
  const line = typeof message === 'string' ? message : String(message ?? '');
  mkdirSync(logsDir, { recursive: true });
  appendFileSync(path.join(logsDir, UPDATER_LOG), `${new Date().toISOString()} ${line}\n`);
}

export function updaterLogPath(logsDir) {
  return path.join(logsDir, UPDATER_LOG);
}
