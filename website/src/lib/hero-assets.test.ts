import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, it } from 'node:test';
import { fileURLToPath } from 'node:url';

import { heroAsset, heroScreenshot, marketingGif, marketingMp4, marketingWebm, ogCard } from './hero-assets.ts';

const assetsDir = path.join(path.dirname(fileURLToPath(import.meta.url)), '../../public/assets');

describe('hero asset URLs', () => {
  it('appends a content hash that changes when the bytes change', () => {
    for (const [filename, url] of [
      ['hero-screenshot.png', heroScreenshot],
      ['marketing.webm', marketingWebm],
      ['marketing.mp4', marketingMp4],
      ['marketing.gif', marketingGif],
      ['og-card.png', ogCard],
    ] as const) {
      const hash = createHash('sha256').update(readFileSync(path.join(assetsDir, filename))).digest('hex').slice(0, 12);
      assert.equal(url, `/assets/${filename}?v=${hash}`);
      assert.equal(heroAsset(filename), url);
    }
  });
});
