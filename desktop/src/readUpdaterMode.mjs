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
