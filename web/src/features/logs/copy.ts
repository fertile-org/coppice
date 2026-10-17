/** User-facing Logs strings. Content owns this list. */
export const LOGS_COPY = {
  nav: 'Logs',
  title: 'Logs',
  source: 'Source',
  sourceAll: 'All',
  sourceServer: 'Server',
  sourceRequests: 'Requests',
  sourceAgentRuns: 'Agent runs',
  sourceDatabase: 'Database',
  level: 'Level',
  levelTrace: 'Trace',
  levelDebug: 'Debug',
  levelInfo: 'Info',
  levelWarn: 'Warning',
  levelError: 'Error',
  time: 'Time',
  from: 'From',
  to: 'To',
  run: 'Run',
  search: 'Search logs',
  live: 'Live',
  empty: 'No logs match these filters.',
  viewLogs: 'View logs',
  loadOlder: 'Load older',
  olderLimit: 'Showing the newest lines only. Narrow the filters or use Export logs to see older ones.',
  loadError: "Couldn't load logs. Try again.",
  loading: 'Loading…',
  export: 'Export logs',
  report: 'Report a problem',
  exportFile: 'coppice-logs.zip',
  exportError: "Couldn't export logs. Try again.",
} as const;

export function savedExport(file: string): string {
  return `Saved ${file}. Coppice removed the tokens and keys it knows about, but check the file before you share it.`;
}

export function reportProblemBody(version: string, osVersion: string): string {
  return [
    `Coppice ${version} on ${osVersion}`,
    'What happened:',
    'What you expected:',
    'Attach the zip from **Export logs** on the Logs page.',
  ].join('\n');
}

export const SOURCE_OPTIONS = [
  { value: '', label: LOGS_COPY.sourceAll },
  { value: 'server', label: LOGS_COPY.sourceServer },
  { value: 'http', label: LOGS_COPY.sourceRequests },
  { value: 'cli', label: LOGS_COPY.sourceAgentRuns },
  { value: 'postgres', label: LOGS_COPY.sourceDatabase },
] as const;

export const LEVEL_OPTIONS = [
  { value: 'trace', label: LOGS_COPY.levelTrace },
  { value: 'debug', label: LOGS_COPY.levelDebug },
  { value: 'info', label: LOGS_COPY.levelInfo },
  { value: 'warn', label: LOGS_COPY.levelWarn },
  { value: 'error', label: LOGS_COPY.levelError },
] as const;

export function sourceLabel(source: string): string {
  return SOURCE_OPTIONS.find((option) => option.value === source)?.label ?? source;
}
