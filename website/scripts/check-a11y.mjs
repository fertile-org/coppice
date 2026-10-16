import path from 'node:path';
import { chromium } from 'playwright';
import AxeBuilder from '@axe-core/playwright';
import { startStaticServer } from './static-server.mjs';

const dist = path.resolve('dist');
const pages = ['/', '/download', '/docs', '/docs/install', '/docs/concepts', '/docs/providers', '/docs/faq'];
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
    const shot = document.querySelector('.hero-shot');
    const fill = document.querySelector('.hero-progress-fill');
    return (
      video &&
      shot &&
      fill &&
      video.readyState >= 2 &&
      video.paused === false &&
      video.videoWidth === 1440 &&
      shot.classList.contains('is-playing') &&
      Number.isFinite(Number.parseFloat(fill.style.width))
    );
  });
  const readProgress = () =>
    home.locator('.hero-shot').evaluate((shot) => {
      const video = shot.querySelector('video.hero-video');
      const fill = shot.querySelector('.hero-progress-fill');
      const bar = shot.querySelector('.hero-progress');
      const videoBox = video.getBoundingClientRect();
      const barBox = bar.getBoundingClientRect();
      const fillBox = fill.getBoundingClientRect();
      const width = Number.parseFloat(fill.style.width);
      const expected = (video.currentTime / video.duration) * 100;
      return {
        paused: video.paused,
        autoplay: video.autoplay,
        currentTime: video.currentTime,
        duration: video.duration,
        width,
        rendered: barBox.width > 0 ? (fillBox.width / barBox.width) * 100 : 0,
        expected,
        pixelWidth: video.videoWidth,
        pixelHeight: video.videoHeight,
        ariaHidden: bar.getAttribute('aria-hidden'),
        text: (bar.textContent || '').trim(),
        display: getComputedStyle(bar).display,
        playingClass: shot.classList.contains('is-playing'),
        barTop: barBox.top - videoBox.top,
        barWidth: barBox.width,
        frameWidth: videoBox.width,
        barHeight: barBox.height,
      };
    });

  const playing = await readProgress();
  await home.waitForTimeout(600);
  const later = await readProgress();
  await home.locator('video.hero-video').evaluate((video) => {
    video.currentTime = Math.max(0, video.duration - 0.2);
  });
  await home.waitForFunction(() => {
    const video = document.querySelector('video.hero-video');
    const fill = document.querySelector('.hero-progress-fill');
    if (!video || video.paused || video.currentTime >= 1 || !fill) return false;
    const width = Number.parseFloat(fill.style.width);
    return Number.isFinite(width) && width < 15;
  });
  const looped = await readProgress();
  await home.close();

  const reduced = await context.newPage();
  await reduced.emulateMedia({ reducedMotion: 'reduce' });
  await reduced.goto(new URL('/', server.url).href, { waitUntil: 'domcontentloaded' });
  await reduced.waitForTimeout(400);
  const still = await reduced.locator('video.hero-video').evaluate((video) => ({
    paused: video.paused,
    autoplay: video.autoplay,
    preload: video.preload,
    currentTime: video.currentTime,
  }));
  const reducedBar = await reduced.locator('.hero-progress').evaluate((bar) => ({
    display: getComputedStyle(bar).display,
    ariaHidden: bar.getAttribute('aria-hidden'),
    text: (bar.textContent || '').trim(),
  }));
  const reducedPlaying = await reduced.locator('.hero-shot').evaluate((shot) =>
    shot.classList.contains('is-playing'),
  );
  await reduced.close();

  const tracksTime = (sample) =>
    Number.isFinite(sample.width) &&
    Math.abs(sample.width - sample.expected) < 8 &&
    Math.abs(sample.rendered - sample.expected) < 8;
  const progressOk =
    playing.ariaHidden === 'true' &&
    playing.text === '' &&
    playing.display === 'block' &&
    playing.playingClass &&
    playing.barHeight > 0 &&
    playing.barHeight <= 4 &&
    Math.abs(playing.barTop) <= 2 &&
    playing.barWidth > playing.frameWidth * 0.9 &&
    tracksTime(playing) &&
    tracksTime(later) &&
    (later.currentTime > playing.currentTime
      ? later.width > playing.width - 1
      : later.currentTime < playing.currentTime) &&
    looped.currentTime < 1 &&
    tracksTime(looped) &&
    looped.width < 15;
  const reducedOk =
    still.paused &&
    still.autoplay === false &&
    !reducedPlaying &&
    reducedBar.display === 'none' &&
    reducedBar.ariaHidden === 'true' &&
    reducedBar.text === '';

  const failed = summary.filter((item) => item.violations.length);
  console.log(JSON.stringify({ summary, playing, later, looped, still, reducedBar }, null, 2));
  if (failed.length || playing.paused || playing.pixelWidth !== 1440 || !progressOk || !reducedOk) {
    console.error('Accessibility check failed.');
    process.exit(1);
  }
  console.log(`color-contrast passed on ${pages.length} pages.`);
} finally {
  await browser.close();
  await server.close();
}
