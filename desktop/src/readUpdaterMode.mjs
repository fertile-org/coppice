// Baked at package time by scripts/dist.mjs. True only when CSC_LINK was set,
// which is the Developer ID signing secret. Unsigned builds stay false and
// the app offers a download link instead of installing in place.
export function readUpdaterMode(jsonText) {
  try {
    const parsed = JSON.parse(jsonText);
    return parsed?.macInPlaceUpdates === true;
  } catch {
    return false;
  }
}

// Null means the package did not record a base, so the app uses the default.
export function readUpdateBase(jsonText) {
  try {
    const value = JSON.parse(jsonText)?.updateBase;
    if (typeof value !== 'string') return null;
    const trimmed = value.trim();
    return trimmed === '' ? null : trimmed;
  } catch {
    return null;
  }
}
