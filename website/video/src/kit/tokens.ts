import { loadFont } from "@remotion/google-fonts/Roboto";

/** Marketing hero canvas (matches Playwright capture viewport). */
export const VIDEO = { width: 1440, height: 900, fps: 30 } as const;

/** Tip of the Coppice app checkout this kit was last refreshed against (branch or SHA). */
export const TOKENS_SOURCE_COMMIT: string = "main-ish-ui-refresh-rmyo9382";

// Blocks rendering until Roboto is ready. Weights match what the app loads.
loadFont("normal", { weights: ["400", "500", "700"], subsets: ["latin"] });
loadFont("italic", { weights: ["400"], subsets: ["latin"] });
