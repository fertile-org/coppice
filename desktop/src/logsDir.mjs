import path from 'node:path';

/**
 * Log directory the desktop app resolves once.
 * Linux: `$XDG_STATE_HOME/Coppice/logs` when that variable is set and non-empty,
 * otherwise `~/.local/state/Coppice/logs`.
 * macOS: `~/Library/Logs/Coppice`.
 */
export function resolveAppLogsDir({ platform, env, home }) {
  if (platform === 'darwin') {
    return path.join(home, 'Library', 'Logs', 'Coppice');
  }
  const stateHome = env.XDG_STATE_HOME;
  if (typeof stateHome === 'string' && stateHome.length > 0) {
    return path.join(stateHome, 'Coppice', 'logs');
  }
  return path.join(home, '.local', 'state', 'Coppice', 'logs');
}

export function desktopServerArgs({ userData, resources, logsDir }) {
  return ['desktop', '--data-dir', userData, '--resources', resources, '--logs-dir', logsDir];
}
