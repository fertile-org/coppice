import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/** Public hero files and the social card. Filenames stay stable; the query changes with the bytes. */
const HERO_FILES = [
  'hero-screenshot.png',
  'hero-screenshot-720.webp',
  'hero-screenshot-1440.webp',
  'hero-screenshot-720.avif',
  'hero-screenshot-1440.avif',
  'marketing.webm',
  'marketing.mp4',
  'marketing.gif',
  'og-card.png',
] as const;

export type HeroFile = (typeof HERO_FILES)[number];

function assetsDir(): string {
  const here = path.dirname(fileURLToPath(import.meta.url));
  const candidates = [
    path.join(here, '../../public/assets'),
    path.join(process.cwd(), 'public/assets'),
    path.join(process.cwd(), 'website/public/assets'),
  ];
  const found = candidates.find((dir) => existsSync(path.join(dir, 'hero-screenshot.png')));
  if (!found) {
    throw new Error(`hero assets not found (looked in ${candidates.join(', ')})`);
  }
  return found;
}

/** `/assets/<file>?v=<sha256 prefix>` so immutable caches drop the previous bytes. */
export function heroAsset(filename: HeroFile): string {
  const bytes = readFileSync(path.join(assetsDir(), filename));
  const hash = createHash('sha256').update(bytes).digest('hex').slice(0, 12);
  return `/assets/${filename}?v=${hash}`;
}

export const heroScreenshot = heroAsset('hero-screenshot.png');
/** The video poster is what visitors see first (the LCP image), so it is the small WebP. */
export const heroPoster = heroAsset('hero-screenshot-1440.webp');
export const heroStillAvif = `${heroAsset('hero-screenshot-720.avif')} 720w, ${heroAsset('hero-screenshot-1440.avif')} 1440w`;
export const heroStillWebp = `${heroAsset('hero-screenshot-720.webp')} 720w, ${heroAsset('hero-screenshot-1440.webp')} 1440w`;
export const marketingWebm = heroAsset('marketing.webm');
export const marketingMp4 = heroAsset('marketing.mp4');
export const marketingGif = heroAsset('marketing.gif');
export const ogCard = heroAsset('og-card.png');
