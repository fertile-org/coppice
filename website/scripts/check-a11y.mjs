import path from 'node:path';
import { chromium } from 'playwright';
import AxeBuilder from '@axe-core/playwright';
import { startStaticServer } from './static-server.mjs';

const dist = path.resolve('dist');
const pages = ['/', '/docs', '/docs/install', '/docs/concepts', '/docs/providers', '/docs/faq'];
const server = await startStaticServer(dist, { style: 'clean' });
const browser = await chromium.launch();
const context = await browser.newContext();
const summary = [];

try {
  for (const urlPath of pages) {
    const page = await context.newPage();
    await page.goto(new URL(urlPath, server.url).href, { waitUntil: 'networkidle' });
    const results = await new AxeBuilder({ page }).withRules(['color-contrast']).analyze();
    const violations = results.violations.map((violation) => ({
      id: violation.id,
      impact: violation.impact,
      help: violation.help,
      nodes: violation.nodes.map((node) => ({
        target: node.target,
        html: node.html,
        failureSummary: node.failureSummary,
      })),
    }));
    summary.push({ url: urlPath, violations });
    await page.close();
  }

  const home = await context.newPage();
  await home.goto(new URL('/', server.url).href, { waitUntil: 'domcontentloaded' });
  await home.waitForFunction(() => {
    const video = document.querySelector('video.hero-video');
    return video && video.readyState >= 2 && video.paused === false;
  });
  const playing = await home.locator('video.hero-video').evaluate((video) => ({
    paused: video.paused,
    currentTime: video.currentTime,
    width: video.videoWidth,
    height: video.videoHeight,
  }));
  await home.close();

  const reduced = await context.newPage();
  await reduced.emulateMedia({ reducedMotion: 'reduce' });
  await reduced.goto(new URL('/', server.url).href, { waitUntil: 'domcontentloaded' });
  await reduced.waitForTimeout(400);
  const still = await reduced.locator('video.hero-video').evaluate((video) => ({
    paused: video.paused,
    preload: video.preload,
    currentTime: video.currentTime,
  }));
  await reduced.close();

  const failed = summary.filter((item) => item.violations.length);
  console.log(JSON.stringify({ summary, playing, still }, null, 2));
  if (failed.length || playing.paused || playing.width !== 1440 || !still.paused) {
    console.error('Accessibility check failed.');
    process.exit(1);
  }
  console.log(`color-contrast passed on ${pages.length} pages.`);
} finally {
  await browser.close();
  await server.close();
}
