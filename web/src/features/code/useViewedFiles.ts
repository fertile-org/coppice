import { useCallback, useEffect, useRef, useState } from 'react';
import { readViewedPaths, writeViewedPaths } from './reviewNavigation';

function readKey(key: string | null): Set<string> {
  return new Set(key ? readViewedPaths(localStorage, key) : []);
}

export function useViewedFiles(key: string | null) {
  const [storedKey, setStoredKey] = useState(key);
  const [viewed, setViewed] = useState<ReadonlySet<string>>(() => readKey(key));
  if (key !== storedKey) {
    setStoredKey(key);
    setViewed(readKey(key));
  }

  const viewedRef = useRef(viewed);
  const keyRef = useRef(key);
  useEffect(() => {
    viewedRef.current = viewed;
    keyRef.current = key;
  }, [viewed, key]);

  const persist = useCallback((next: Set<string>) => {
    viewedRef.current = next;
    const storageKey = keyRef.current;
    if (storageKey) writeViewedPaths(localStorage, storageKey, [...next]);
    setViewed(next);
    return next;
  }, []);

  const setViewedPath = useCallback(
    (path: string, on: boolean) => {
      const next = new Set(viewedRef.current);
      if (on) next.add(path);
      else next.delete(path);
      return persist(next);
    },
    [persist],
  );

  const toggleViewed = useCallback(
    (path: string) => {
      const next = new Set(viewedRef.current);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return persist(next);
    },
    [persist],
  );

  return { viewed, setViewedPath, toggleViewed };
}
