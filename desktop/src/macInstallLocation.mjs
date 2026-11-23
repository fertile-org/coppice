/**
 * True when this macOS install cannot receive an in-place zip update
 * (DMG / App Translocation / outside Applications).
 */
export function macInstallBlocked({ execPath, isInApplicationsFolder }) {
  const path = typeof execPath === 'string' ? execPath : '';
  if (path.includes('/AppTranslocation/')) return true;
  if (path.startsWith('/Volumes/')) return true;
  if (isInApplicationsFolder === false) return true;
  return false;
}
