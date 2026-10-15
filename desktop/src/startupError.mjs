export const DATABASE_STILL_RUNNING =
  "Coppice couldn't start because an earlier Coppice database may still be running. Quit any other Coppice window, or restart your computer, then open Coppice again.";

/** The sentence the server prints when a leftover postmaster blocks startup. */
export function startupFailureMessage(lines, fallback) {
  if (lines.some((line) => line.includes(DATABASE_STILL_RUNNING))) {
    return DATABASE_STILL_RUNNING;
  }
  return fallback;
}
