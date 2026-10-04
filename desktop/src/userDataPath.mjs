import path from 'node:path';

// Unpackaged runs get their own data dir so they never share the installed
// app's Postgres cluster or single-instance lock. Null keeps the OS default.
export function devUserDataPath({ isPackaged, env, appData }) {
  if (isPackaged) return null;
  if (env.COPPICE_DESKTOP_USER_DATA) return path.resolve(env.COPPICE_DESKTOP_USER_DATA);
  return path.join(appData, 'Coppice-dev');
}
