// Renders public/assets/og-card.png (1200×630), the social preview card.
// Not part of the build: run `npm run og:image` after changing the logo,
// the hero screenshot, or the card copy below, and commit the PNG.
// Uses Playwright's Chromium, or the system Chrome when that is not installed.
import fs from 'node:fs';
import path from 'node:path';
import { chromium } from 'playwright';

const root = path.resolve(import.meta.dirname, '..');
const out = path.join(root, 'public/assets/og-card.png');

const dataUri = (file, type) => `data:${type};base64,${fs.readFileSync(path.join(root, file)).toString('base64')}`;
const font = (pkg, file) => dataUri(`node_modules/@fontsource-variable/${pkg}/files/${file}`, 'font/woff2');

const html = `<!doctype html>
<html>
<head>
<meta charset="utf-8" />
<style>
  @font-face { font-family: Fraunces; font-weight: 100 900; src: url(${font('fraunces', 'fraunces-latin-wght-normal.woff2')}) format('woff2'); }
  @font-face { font-family: 'Source Sans 3'; font-weight: 200 900; src: url(${font('source-sans-3', 'source-sans-3-latin-wght-normal.woff2')}) format('woff2'); }
  * { box-sizing: border-box; margin: 0; }
  body { width: 1200px; height: 630px; overflow: hidden; background: #faf7f2; color: #2a1f18; font-family: 'Source Sans 3', sans-serif; }
  .card { position: relative; width: 100%; height: 100%; padding: 64px 0 0 72px; }
  .brand { display: flex; align-items: center; gap: 16px; font-family: Fraunces, serif; font-size: 40px; font-weight: 600; }
  .brand img { width: 56px; height: 59px; }
  h1 { margin-top: 56px; width: 560px; font-family: Fraunces, serif; font-size: 62px; line-height: 1.05; font-weight: 600; letter-spacing: -0.01em; }
  p { margin-top: 24px; width: 500px; font-size: 27px; line-height: 1.35; color: #4a382c; }
  .tags { position: absolute; left: 72px; bottom: 56px; font-size: 22px; color: #2d5a3d; font-weight: 600; }
  .shot { position: absolute; top: 96px; left: 680px; width: 720px; border-radius: 14px; border: 1px solid #e4d9cc; box-shadow: 0 16px 40px rgba(42, 31, 24, 0.14); }
</style>
</head>
<body>
  <div class="card">
    <div class="brand"><img src="${dataUri('public/logo.svg', 'image/svg+xml')}" alt="" />Coppice</div>
    <h1>The AI coding agent board</h1>
    <p>Agents plan, build and review on one board. Nothing is done until you accept the diff.</p>
    <div class="tags">Claude Code · Codex · Cursor · OpenCode · and more</div>
    <img class="shot" src="${dataUri('public/assets/hero-screenshot.png', 'image/png')}" alt="" />
  </div>
</body>
</html>`;

async function launch() {
  try {
    return await chromium.launch();
  } catch {
    return chromium.launch({ channel: 'chrome' });
  }
}

const browser = await launch();
try {
  const page = await browser.newPage({ viewport: { width: 1200, height: 630 } });
  await page.setContent(html, { waitUntil: 'load' });
  await page.evaluate(() => document.fonts.ready);
  fs.mkdirSync(path.dirname(out), { recursive: true });
  await page.screenshot({ path: out, type: 'png' });
  console.log(`wrote ${path.relative(root, out)}`);
} finally {
  await browser.close();
}
