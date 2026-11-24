#!/usr/bin/env node
/**
 * Render the Hero composition and publish marketing assets.
 *
 * Writes:
 *   website/public/assets/marketing.mp4
 *   website/public/assets/marketing.webm
 *   website/public/assets/marketing.gif   (960px, ≤ 2.5 MB)
 *   static/marketing.gif
 *   website/public/assets/hero-screenshot.png  (Done-frame still)
 * then runs `npm run web:images` for WebP/AVIF derivatives.
 *
 * Not run in CI — commit the binaries after a local `make hero-video`.
 */
import { spawn } from 'node:child_process';
import { copyFile, mkdir, stat } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const VIDEO_ROOT = path.resolve(HERE, '..');
const REPO_ROOT = path.resolve(VIDEO_ROOT, '../..');
const ASSETS = path.join(REPO_ROOT, 'website', 'public', 'assets');
const OUT = path.join(VIDEO_ROOT, 'out');
const GIF_MAX_BYTES = Math.floor(2.5 * 1024 * 1024);
const POSTER_FRAME = 235; // Done hold near the end of Accept beat
// Remotion ships a trimmed ffmpeg with the GIF filters this script needs.
const FFMPEG = path.join(
  VIDEO_ROOT,
  'node_modules/@remotion/compositor-linux-x64-gnu/ffmpeg',
);
const FFPROBE = path.join(
  VIDEO_ROOT,
  'node_modules/@remotion/compositor-linux-x64-gnu/ffprobe',
);

function run(cmd, args, opts = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(cmd, args, { stdio: 'inherit', ...opts });
    child.on('error', reject);
    child.on('exit', (code) => {
      if (code === 0) resolve();
      else reject(new Error(`${cmd} ${args.join(' ')} exited ${code}`));
    });
  });
}

function runCapture(cmd, args, opts = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(cmd, args, { ...opts, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (d) => {
      stdout += d;
    });
    child.stderr.on('data', (d) => {
      stderr += d;
    });
    child.on('error', reject);
    child.on('exit', (code) => {
      if (code === 0) resolve({ stdout, stderr });
      else reject(new Error(`${cmd} failed (${code}): ${stderr || stdout}`));
    });
  });
}

async function main() {
  await mkdir(OUT, { recursive: true });
  await mkdir(ASSETS, { recursive: true });

  const mp4Tmp = path.join(OUT, 'hero.mp4');
  const webmTmp = path.join(OUT, 'hero.webm');
  const posterTmp = path.join(OUT, 'hero-poster.png');
  const gifTmp = path.join(OUT, 'hero.gif');

  console.log('hero-video: typecheck + lint');
  await run('npm', ['run', 'lint'], { cwd: VIDEO_ROOT });

  console.log('hero-video: render mp4');
  await run(
    'npx',
    ['remotion', 'render', 'Hero', mp4Tmp, '--codec=h264', '--image-format=jpeg'],
    { cwd: VIDEO_ROOT },
  );

  console.log('hero-video: render webm');
  await run(
    'npx',
    ['remotion', 'render', 'Hero', webmTmp, '--codec=vp8', '--image-format=jpeg'],
    { cwd: VIDEO_ROOT },
  );

  console.log(`hero-video: poster still @ frame ${POSTER_FRAME}`);
  await run(
    'npx',
    ['remotion', 'still', 'Hero', posterTmp, `--frame=${POSTER_FRAME}`],
    { cwd: VIDEO_ROOT },
  );

  console.log('hero-video: encode 960px gif');
  // Remotion's ffmpeg has no `fps` filter — use `-r`. Palette keeps size ≤ 2.5 MB.
  await run(FFMPEG, [
    '-y',
    '-i',
    mp4Tmp,
    '-r',
    '8',
    '-vf',
    'scale=960:-1:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=64:stats_mode=diff[p];[s1][p]paletteuse=dither=bayer:bayer_scale=5',
    '-loop',
    '0',
    gifTmp,
  ]);

  const gifBytes = (await stat(gifTmp)).size;
  if (gifBytes > GIF_MAX_BYTES) {
    console.log(`hero-video: gif ${gifBytes} bytes > 2.5 MB, retrying with fewer colours`);
    await run(FFMPEG, [
      '-y',
      '-i',
      mp4Tmp,
      '-r',
      '6',
      '-vf',
      'scale=960:-1:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=48:stats_mode=diff[p];[s1][p]paletteuse=dither=bayer:bayer_scale=5',
      '-loop',
      '0',
      gifTmp,
    ]);
  }
  const gifFinal = (await stat(gifTmp)).size;
  if (gifFinal > GIF_MAX_BYTES) {
    throw new Error(`GIF is ${(gifFinal / 1024 / 1024).toFixed(2)} MB (max 2.5 MB)`);
  }

  const mp4Out = path.join(ASSETS, 'marketing.mp4');
  const webmOut = path.join(ASSETS, 'marketing.webm');
  const gifSite = path.join(ASSETS, 'marketing.gif');
  const gifReadme = path.join(REPO_ROOT, 'static', 'marketing.gif');
  const posterOut = path.join(ASSETS, 'hero-screenshot.png');

  await copyFile(mp4Tmp, mp4Out);
  await copyFile(webmTmp, webmOut);
  await copyFile(gifTmp, gifSite);
  await copyFile(gifTmp, gifReadme);
  await copyFile(posterTmp, posterOut);

  console.log('hero-video: web:images');
  await run('npm', ['run', 'web:images'], { cwd: path.join(REPO_ROOT, 'website') });

  const probe = await runCapture(FFPROBE, [
    '-v',
    'error',
    '-select_streams',
    'v:0',
    '-show_entries',
    'stream=width,height,duration',
    '-of',
    'json',
    mp4Out,
  ]);
  const meta = JSON.parse(probe.stdout);
  const stream = meta.streams?.[0] ?? {};
  const seconds = Number(stream.duration);
  if (!(seconds >= 5 && seconds <= 10)) {
    throw new Error(`hero duration ${seconds}s is outside 5–10s`);
  }
  if (Number(stream.width) !== 1440 || Number(stream.height) !== 900) {
    throw new Error(`hero size ${stream.width}x${stream.height}, expected 1440x900`);
  }

  console.log(
    `hero-video: done mp4=${(await stat(mp4Out)).size} webm=${(await stat(webmOut)).size} gif=${gifFinal} ${seconds.toFixed(2)}s`,
  );
}

main().catch((err) => {
  console.error(err instanceof Error ? err.stack ?? err.message : String(err));
  process.exit(1);
});
