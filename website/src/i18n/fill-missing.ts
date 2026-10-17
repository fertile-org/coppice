function isPlainObject(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value);
}

/** Copy `base`, replacing only the keys `extra` actually sets. Arrays merge by index. */
export function fillMissing<T>(base: T, extra: unknown): T {
  if (extra === undefined || extra === null) return base;
  if (Array.isArray(base)) {
    if (!Array.isArray(extra)) return base;
    const length = Math.max(base.length, extra.length);
    const merged = [];
    for (let index = 0; index < length; index += 1) {
      if (index >= extra.length) merged.push(base[index]);
      else if (index >= base.length) merged.push(extra[index]);
      else merged.push(fillMissing(base[index], extra[index]));
    }
    return merged as T;
  }
  if (isPlainObject(base) && isPlainObject(extra)) {
    const merged: Record<string, unknown> = { ...base };
    for (const key of Object.keys(extra)) {
      const value = extra[key];
      if (value === undefined) continue;
      merged[key] = key in base ? fillMissing(base[key], value) : value;
    }
    return merged as T;
  }
  return extra as T;
}
