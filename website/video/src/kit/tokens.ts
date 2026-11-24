import { loadFont } from "@remotion/google-fonts/Roboto";

/** Marketing hero canvas (matches Playwright capture viewport). */
export const VIDEO = { width: 1440, height: 900, fps: 30 } as const;

/** Coppice app commit the kit chrome was last checked against (Chat-above-Agents nav). */
export const TOKENS_SOURCE_COMMIT: string =
  "986cf5e0610f08ddcf27744b50faf8265ea1eec5";

// Blocks rendering until Roboto is ready. Weights match what the app loads.
loadFont("normal", { weights: ["400", "500", "700"], subsets: ["latin"] });
loadFont("italic", { weights: ["400"], subsets: ["latin"] });
