import { mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';

export const UPDATE_SETTINGS_NAME = 'update-settings.json';

const DEFAULTS = { automaticUpdates: true };

export function readUpdateSettings(file, fsImpl = { readFileSync }) {
  try {
    const parsed = JSON.parse(fsImpl.readFileSync(file, 'utf8'));
    if (parsed && typeof parsed.automaticUpdates === 'boolean') {
      return { automaticUpdates: parsed.automaticUpdates };
    }
  } catch {
    // Missing or unreadable files keep the default: checks on.
  }
  return { ...DEFAULTS };
}

export function writeUpdateSettings(file, settings, fsImpl) {
  const io = fsImpl ?? { mkdirSync, writeFileSync, renameSync };
  const next = { automaticUpdates: settings?.automaticUpdates === true };
  io.mkdirSync(dirname(file), { recursive: true });
  const tmp = `${file}.tmp`;
  io.writeFileSync(tmp, `${JSON.stringify(next)}\n`);
  io.renameSync(tmp, file);
  return next;
}
