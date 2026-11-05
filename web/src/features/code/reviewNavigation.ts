/** Browser-only review progress. A new head SHA is a new key, so the set resets. */
export function viewedStorageKey(
  repoId: string,
  worktreePath: string,
  headSha: string,
): string {
  return `coppice.codeReview.viewed\0${repoId}\0${worktreePath}\0${headSha}`;
}

export function splitFilePath(path: string): { dir: string; name: string } {
  const slash = path.lastIndexOf('/');
  if (slash < 0) return { dir: '', name: path };
  return { dir: path.slice(0, slash + 1), name: path.slice(slash + 1) };
}

export function readViewedPaths(storage: Storage, key: string): string[] {
  try {
    const raw = storage.getItem(key);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((item): item is string => typeof item === 'string');
  } catch {
    return [];
  }
}

export function writeViewedPaths(
  storage: Storage,
  key: string,
  paths: readonly string[],
): void {
  try {
    storage.setItem(key, JSON.stringify(paths));
  } catch {
    /* private mode or quota */
  }
}

/** Stay on the current file at either end. */
export function stepFile(
  paths: readonly string[],
  current: string | undefined,
  delta: 1 | -1,
): string | undefined {
  if (paths.length === 0) return undefined;
  const index = current ? paths.indexOf(current) : -1;
  if (index < 0) return paths[0];
  const next = index + delta;
  if (next < 0 || next >= paths.length) return current;
  return paths[next];
}

/** Next unviewed file after `current`, wrapping. Stays put when every file is viewed. */
export function nextUnviewed(
  paths: readonly string[],
  viewed: ReadonlySet<string>,
  current: string | undefined,
): string | undefined {
  if (paths.length === 0) return undefined;
  const start = current ? paths.indexOf(current) : -1;
  const origin = start < 0 ? 0 : start;
  for (let step = 1; step <= paths.length; step += 1) {
    const path = paths[(origin + step) % paths.length];
    if (path && !viewed.has(path)) return path;
  }
  return current ?? paths[0];
}
