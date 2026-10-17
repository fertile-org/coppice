export const DATABASE_STILL_RUNNING =
  "Coppice couldn't start because an earlier Coppice database may still be running. Quit any other Coppice window, or restart your computer, then open Coppice again.";

export const APP_PASSWORD_RESET_FAILED =
  "Coppice couldn't start because it couldn't set a new database password. Quit and open Coppice again.";

/** The sentence the server prints when startup can tell the user what to do. */
export function startupFailureMessage(lines, fallback) {
  for (const message of [DATABASE_STILL_RUNNING, APP_PASSWORD_RESET_FAILED]) {
    if (lines.some((line) => line.includes(message))) return message;
  }
  return fallback;
}
