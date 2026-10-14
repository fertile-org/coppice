/** OS group order for the download page. No CPU architecture detection. */

export const noJsOsOrder = ['mac', 'linux', 'windows'] as const;

export type OsId = (typeof noJsOsOrder)[number];

/** No-JS order, with the visitor's OS group moved first. Rows inside a group stay put. */
export function orderOs(detected: OsId | null): OsId[] {
  if (!detected) return [...noJsOsOrder];
  return [detected, ...noJsOsOrder.filter((os) => os !== detected)];
}

/**
 * OS only. Apple Silicon and Intel are not distinguished: Mac user agents
 * often say Intel either way.
 */
export function detectOs(userAgent: string): OsId | null {
  if (/Android|iPhone|iPad|iPod/i.test(userAgent)) return null;
  if (/Windows/i.test(userAgent)) return 'windows';
  if (/Macintosh|Mac OS X/i.test(userAgent)) return 'mac';
  if (/Linux/i.test(userAgent)) return 'linux';
  return null;
}
