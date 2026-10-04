const READY_RE = /^COPPICE_READY url=(http:\/\/127\.0\.0\.1:\d+)\s*$/;

export function parseReadyLine(line) {
  const match = READY_RE.exec(line);
  return match ? match[1] : null;
}
