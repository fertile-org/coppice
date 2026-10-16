#!/usr/bin/env node
/**
 * Homepage and README hero.
 *
 * Same stack as `make screenshot`: Compose project coppice-screenshot,
 * desktop mode, 1440×900. Seeds the acme-web repo and a small board, then
 * records the seven-scene storyboard. Writes:
 *
 *   website/public/assets/marketing.mp4   1440×900
 *   website/public/assets/marketing.webm  1440×900
 *   website/public/assets/marketing.gif   960px wide
 *   static/marketing.gif                  same GIF
 *   website/public/assets/hero-screenshot.png
 *   static/screenshots/01-… through 07-…
 *
 * Run after the screenshot stack is up:
 *   node e2e/screenshot/capture-hero.mjs
 */

import { copyFile, mkdir, mkdtemp, readFile, rm, stat, writeFile } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { resetMarketingWorkspace } from './seed.mjs';

const API = process.env.COPPICE_API_URL ?? 'http://localhost:5000';
const WEB = process.env.COPPICE_WEB_URL ?? 'http://localhost:5001';
const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, '../..');
const ASSETS = path.join(REPO_ROOT, 'website', 'public', 'assets');
const STILLS_DIR = path.join(REPO_ROOT, 'static', 'screenshots');
const GIF_SITE = path.join(ASSETS, 'marketing.gif');
const GIF_README = path.join(REPO_ROOT, 'static', 'marketing.gif');
const MP4_OUT = path.join(ASSETS, 'marketing.mp4');
const WEBM_OUT = path.join(ASSETS, 'marketing.webm');
const POSTER = path.join(ASSETS, 'hero-screenshot.png');
const PROJECT = process.env.COPPICE_COMPOSE_PROJECT ?? 'coppice-screenshot';
const COMPOSE_FILES = [
  'deploy/docker-compose.yml',
  'deploy/docker-compose.screenshot.yml',
];

const VIEWPORT = { width: 1440, height: 900 };
const GIF_WIDTH = 960;
const REPO_NAME = 'acme-web';
const REPO_PATH = '/tmp/coppice-screenshot-repo';
const TICKET_TITLE = 'Add a dark mode toggle to Settings';
const LINE_COMMENT = 'Use the existing theme hook.';

const CAPTIONS = [
  'Write a ticket.',
  'Your PM agent drafts a plan.',
  'You approve it.',
  'Agents build, review, and test.',
  'Review the diff. Comment on any line.',
  'Nothing is done until you accept it.',
];

const PLAN_BODY = `## Plan

### Approach
Add a dark mode toggle on the Settings page. Keep the choice on this machine and apply it through the theme hook the rest of the app already uses.

### Steps
- [ ] Add the Settings toggle
- [ ] Use the existing theme hook
- [ ] Add a settings test

### Risks
None.

### Out of scope
A new color palette.`;

const SUPPORTING = [
  {
    title: 'Remember the last open settings tab',
    description: 'Reopen Settings on the tab the operator was using.',
    status: 'backlog',
    priority: 'low',
    agent: 'PM',
  },
  {
    title: 'Tighten spacing on the settings form',
    description: 'The fields sit too far apart on a laptop window.',
    status: 'in_progress',
    priority: 'high',
    agent: 'Frontend',
    live: true,
  },
  {
    title: 'Review the settings page at a narrow width',
    description: 'Check the layout when the window is about 800 pixels wide.',
    status: 'in_review',
    priority: 'medium',
    agent: 'QA',
  },
  {
    title: 'Cover theme switching with a smoke test',
    description: 'Open Settings, switch the theme, and reload.',
    status: 'in_qa',
    priority: 'medium',
    agent: 'QA',
  },
  {
    title: 'Publish the theme token notes',
    description: 'Write down the color tokens the theme hook reads.',
    status: 'done',
    priority: 'low',
    agent: 'Frontend',
  },
];

const FORBIDDEN =
  /\b(mock|fake|jira|linear|asana|trello|github|gitlab|cursor|claude|codex|opencode|kilo|copilot|devin|windsurf)\b/i;

const MAX_HEALTH_ATTEMPTS = 90;

function fail(message) {
  console.error(`hero: ${message}`);
  process.exit(1);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function run(args, { input, cwd = REPO_ROOT } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(args[0], args.slice(1), {
      cwd,
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => {
      stdout += chunk;
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.on('error', reject);
    child.on('close', (code) => {
      if (code !== 0) {
        reject(new Error(`${args.join(' ')} failed (${code}): ${stderr || stdout}`));
        return;
      }
      resolve(stdout);
    });
    if (input !== undefined) child.stdin.end(input);
    else child.stdin.end();
  });
}

function composeArgs(extra) {
  const args = ['docker', 'compose', '-p', PROJECT];
  for (const file of COMPOSE_FILES) args.push('-f', file);
  args.push(...extra);
  return args;
}

function serverUser() {
  const uid = process.env.COPPICE_UID ?? String(process.getuid?.() ?? 1000);
  const gid = process.env.COPPICE_GID ?? String(process.getgid?.() ?? 1000);
  return `${uid}:${gid}`;
}

function parseSessionCookie(setCookieHeaders) {
  for (const header of setCookieHeaders) {
    const match = /coppice_session=([^;]+)/.exec(header);
    if (match) return match[1];
  }
  return null;
}

async function waitForOk(url, label) {
  for (let attempt = 1; attempt <= MAX_HEALTH_ATTEMPTS; attempt += 1) {
    try {
      const res = await fetch(url);
      if (res.ok) return;
    } catch {
      // not ready yet
    }
    await sleep(1000);
  }
  fail(`${label} not ready at ${url}`);
}

async function requireDesktopMode() {
  const res = await fetch(`${API}/api/auth/capabilities`);
  if (!res.ok) fail(`capabilities failed: ${res.status} ${await res.text()}`);
  const body = await res.json();
  if (body.desktopMode !== true) {
    fail('server is not in desktop_mode. Run `make screenshot` first.');
  }
}

async function desktopSession() {
  const res = await fetch(`${API}/api/auth/desktop-session`, { method: 'POST' });
  if (!res.ok) fail(`desktop-session failed: ${res.status} ${await res.text()}`);
  const setCookie =
    typeof res.headers.getSetCookie === 'function'
      ? res.headers.getSetCookie()
      : [res.headers.get('set-cookie')].filter(Boolean);
  const sessionToken = parseSessionCookie(setCookie);
  if (!sessionToken) fail('desktop-session did not return coppice_session');
  const body = await res.json();
  if (!body.csrfToken) fail('desktop-session did not return csrfToken');
  return { cookie: `coppice_session=${sessionToken}`, csrfToken: body.csrfToken };
}

async function api(method, apiPath, auth, body) {
  const headers = { cookie: auth.cookie };
  if (body !== undefined) headers['content-type'] = 'application/json';
  if (method !== 'GET') headers['x-csrf-token'] = auth.csrfToken;
  const res = await fetch(`${API}${apiPath}`, {
    method,
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    fail(`${method} ${apiPath} failed: ${res.status} ${await res.text()}`);
  }
  if (res.status === 204) return null;
  const text = await res.text();
  if (!text) return null;
  return JSON.parse(text);
}

async function psql(sql) {
  await run(
    composeArgs([
      'exec',
      '-T',
      'postgres',
      'psql',
      '-U',
      'coppice',
      '-d',
      'coppice',
      '-v',
      'ON_ERROR_STOP=1',
      '-q',
    ]),
    { input: sql },
  );
}

function sqlQuote(value) {
  return `'${String(value).replaceAll("'", "''")}'`;
}

async function heroGit(mode, short) {
  const script = await readFile(path.join(HERE, 'seed-hero-repo.sh'), 'utf8');
  const args = ['exec', '-T', '-u', serverUser(), 'server', 'sh', '-s', '--', mode];
  if (short) args.push(short);
  const stdout = await run(composeArgs(args), { input: script });
  return stdout
    .trim()
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .at(-1);
}

async function seedAgents(auth) {
  const existing = await api('GET', '/api/agents', auth);
  for (const agent of existing.items ?? []) {
    await api('DELETE', `/api/agents/${agent.id}`, auth);
  }
  const presets = await api('GET', '/api/agent-presets', auth);
  const byKey = new Map((presets.items ?? []).map((preset) => [preset.key, preset]));
  const wanted = [
    ['PM', 'pm'],
    ['Frontend', 'frontend_engineer'],
    ['QA', 'qc'],
  ];
  const ids = {};
  for (const [name, key] of wanted) {
    const preset = byKey.get(key);
    if (!preset) fail(`preset ${key} is missing`);
    const created = await api('POST', '/api/agents', auth, {
      name,
      presetId: preset.id,
      connector: 'claude-code',
      enabled: true,
    });
    ids[name] = created.id;
  }
  return ids;
}

async function seedSupporting(auth, agentIds, boardId) {
  let liveTicketId = null;
  for (const ticket of SUPPORTING) {
    const created = await api('POST', `/api/boards/${boardId}/tickets`, auth, {
      title: ticket.title,
      description: ticket.description,
      priority: ticket.priority,
    });
    if (ticket.status === 'in_progress') {
      await api('PATCH', `/api/tickets/${created.id}`, auth, { skipPlanning: true });
    }
    if (ticket.status === 'done') {
      await api('PATCH', `/api/tickets/${created.id}/status`, auth, {
        status: 'wait_for_final_review',
      });
      await api('POST', `/api/tickets/${created.id}/final-approve`, auth, {});
    } else if (ticket.status !== 'backlog') {
      await api('PATCH', `/api/tickets/${created.id}/status`, auth, {
        status: ticket.status,
      });
    }
    await api('POST', `/api/tickets/${created.id}/assign`, auth, {
      agentId: agentIds[ticket.agent],
    });
    if (ticket.live) liveTicketId = created.id;
  }
  if (!liveTicketId) fail('missing the in-progress ticket');
  await psql(`
    INSERT INTO agent_runs (
      id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
      context_profile, started_at
    ) VALUES (
      gen_random_uuid(),
      ${sqlQuote(liveTicketId)}::uuid,
      ${sqlQuote(agentIds.Frontend)}::uuid,
      'work_on_ticket',
      'running',
      'permissive-default',
      'full',
      now() - interval '2 minutes'
    );
  `);
}

async function seedWorkspace(auth) {
  await resetMarketingWorkspace();
  const agentIds = await seedAgents(auth);
  await heroGit('init');
  const repo = await api('POST', '/api/repos', auth, {
    name: REPO_NAME,
    localPath: REPO_PATH,
    defaultBranch: 'main',
  });
  if (repo.verificationStatus !== 'ready') {
    fail(`repo is not ready: ${repo.verificationStatus} ${repo.verificationError ?? ''}`);
  }
  const board = await api('POST', '/api/boards', auth, { name: REPO_NAME });
  await seedSupporting(auth, agentIds, board.id);
  console.log(`hero: seeded board ${board.id} repo ${repo.id}`);
  return { agentIds, repoId: repo.id, boardId: board.id };
}

async function attachPlan(ticketId, agentId) {
  await psql(`
    INSERT INTO ticket_comments (
      id, ticket_id, author_type, author_id, body, intent,
      mentions, attachment_ids, plan_content_version
    ) VALUES (
      gen_random_uuid(),
      ${sqlQuote(ticketId)}::uuid,
      'agent',
      ${sqlQuote(agentId)}::uuid,
      $plan$${PLAN_BODY}$plan$,
      'plan',
      '[]'::jsonb,
      '{}',
      (SELECT content_version FROM tickets WHERE id = ${sqlQuote(ticketId)}::uuid)
    );
  `);
}

async function launchBrowser() {
  try {
    return await chromium.launch({
      headless: true,
      args: ['--disable-dev-shm-usage'],
    });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.log(`hero: bundled Chromium failed (${message.split('\n')[0]}). Trying Google Chrome.`);
    return chromium.launch({
      channel: 'chrome',
      headless: true,
      args: ['--disable-dev-shm-usage'],
    });
  }
}

async function preparePage(page) {
  await page
    .waitForFunction(() => document.fonts.check('16px Roboto'), null, { timeout: 8_000 })
    .catch(() => {});
  await page.evaluate(() => document.fonts.ready);
  await page.addStyleTag({
    content:
      'html,body{overflow:hidden!important}*{scrollbar-width:none!important}*{caret-color:transparent!important}::-webkit-scrollbar{display:none!important}',
  });
}

async function assertClean(page, label) {
  const bodyText = await page.locator('body').innerText();
  if (/sign out/i.test(bodyText) || bodyText.includes('admin@localhost')) {
    fail(`${label}: account chrome is visible`);
  }
  const hit = bodyText.match(FORBIDDEN);
  if (hit) {
    fail(`${label}: forbidden word ${hit[0]} in ${JSON.stringify(bodyText.slice(0, 700))}`);
  }
  if (bodyText.includes('/data/') || /Technical Lead|couldn't start/.test(bodyText)) {
    fail(`${label}: internal note is visible: ${JSON.stringify(bodyText.slice(0, 700))}`);
  }
}

async function setCaption(page, text) {
  if (text && !CAPTIONS.includes(text)) fail(`caption is not in the storyboard: ${text}`);
  await page.evaluate(
    ({ caption, styleText }) => {
      sessionStorage.setItem('coppice.hero.caption', caption ?? '');
      let style = document.getElementById('coppice-hero-caption-style');
      if (!style) {
        style = document.createElement('style');
        style.id = 'coppice-hero-caption-style';
        style.textContent = styleText;
        document.head.appendChild(style);
      }
      let bar = document.getElementById('coppice-hero-caption');
      if (!bar) {
        bar = document.createElement('div');
        bar.id = 'coppice-hero-caption';
        document.body.appendChild(bar);
      }
      bar.textContent = caption ?? '';
      bar.style.display = caption ? 'block' : 'none';
    },
    { caption: text ?? '', styleText: CAPTION_STYLE },
  );
  if (text) {
    const shown = await page.locator('#coppice-hero-caption').innerText();
    if (shown !== text) fail(`caption on screen was ${JSON.stringify(shown)}`);
    const box = await page.locator('#coppice-hero-caption').boundingBox();
    if (!box || box.width < 80 || box.y < 700) {
      fail(`caption box is ${JSON.stringify(box)}`);
    }
  }
}

const CAPTION_STYLE = `
  #coppice-hero-caption {
    position: fixed;
    left: 50%;
    bottom: 28px;
    transform: translateX(-50%);
    z-index: 2147483646;
    max-width: calc(100% - 64px);
    margin: 0;
    padding: 12px 22px;
    border-radius: 999px;
    background: rgba(28, 25, 23, 0.94);
    color: #faf7f2;
    font: 600 28px/1.25 Roboto, ui-sans-serif, system-ui, sans-serif;
    letter-spacing: -0.015em;
    box-shadow: 0 10px 28px rgba(28, 25, 23, 0.28);
    pointer-events: none;
    white-space: nowrap;
  }
`;

async function installCaption(page) {
  const current = await page.evaluate(
    () => sessionStorage.getItem('coppice.hero.caption') ?? '',
  );
  await setCaption(page, current || null);
}

async function shoot(page, name) {
  await mkdir(STILLS_DIR, { recursive: true });
  const file = path.join(STILLS_DIR, `${name}.png`);
  await page.screenshot({ path: file, type: 'png', animations: 'disabled' });
  console.log(`hero: wrote ${file}`);
  return file;
}

async function scrollBoardTo(page, label) {
  await page.locator('#board').evaluate((el, name) => {
    const col = document.querySelector(`[aria-label="${name}"]`);
    if (!col) return;
    const board = el.getBoundingClientRect();
    const colBox = col.getBoundingClientRect();
    el.scrollTo({
      left: el.scrollLeft + colBox.left - board.left - 4,
      behavior: 'smooth',
    });
  }, label);
  await sleep(900);
}

function ffmpeg(args) {
  return new Promise((resolve, reject) => {
    const child = spawn('ffmpeg', args, { stdio: ['ignore', 'ignore', 'pipe'] });
    let stderr = '';
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.on('error', (error) => {
      if (error.code === 'ENOENT') reject(new Error('ffmpeg is required'));
      else reject(error);
    });
    child.on('close', (code) => {
      if (code !== 0) reject(new Error(`ffmpeg failed (${code}): ${stderr}`));
      else resolve();
    });
  });
}

function ffprobe(file) {
  return new Promise((resolve, reject) => {
    const child = spawn(
      'ffprobe',
      [
        '-v',
        'error',
        '-select_streams',
        'v:0',
        '-show_entries',
        'format=duration:stream=width,height,duration',
        '-of',
        'json',
        file,
      ],
      { stdio: ['ignore', 'pipe', 'pipe'] },
    );
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => {
      stdout += chunk;
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.on('error', reject);
    child.on('close', (code) => {
      if (code !== 0) reject(new Error(`ffprobe failed (${code}): ${stderr}`));
      else {
        const parsed = JSON.parse(stdout);
        const stream = parsed.streams?.[0] ?? {};
        const duration = stream.duration ?? parsed.format?.duration;
        resolve({ ...stream, duration });
      }
    });
  });
}

async function encode(raw, startSec, endSec) {
  const duration = Math.max(0.1, endSec - startSec);
  console.log(`hero: trimming ${startSec.toFixed(2)}s → ${endSec.toFixed(2)}s (${duration.toFixed(2)}s)`);
  const ss = startSec.toFixed(3);
  const to = endSec.toFixed(3);
  await mkdir(ASSETS, { recursive: true });
  await ffmpeg([
    '-y',
    '-ss',
    ss,
    '-to',
    to,
    '-i',
    raw,
    '-an',
    '-c:v',
    'libx264',
    '-pix_fmt',
    'yuv420p',
    '-crf',
    '20',
    '-preset',
    'medium',
    '-movflags',
    '+faststart',
    MP4_OUT,
  ]);
  await ffmpeg([
    '-y',
    '-ss',
    ss,
    '-to',
    to,
    '-i',
    raw,
    '-an',
    '-c:v',
    'libvpx-vp9',
    '-crf',
    '34',
    '-b:v',
    '0',
    '-pix_fmt',
    'yuv420p',
    '-row-mt',
    '1',
    WEBM_OUT,
  ]);

  const scratch = await mkdtemp(path.join(tmpdir(), 'coppice-hero-gif-'));
  const palette = path.join(scratch, 'palette.png');
  const filters = `fps=6,scale=${GIF_WIDTH}:-1:flags=lanczos`;
  await ffmpeg([
    '-y',
    '-ss',
    ss,
    '-to',
    to,
    '-i',
    raw,
    '-vf',
    `${filters},palettegen=max_colors=96:stats_mode=diff`,
    palette,
  ]);
  await ffmpeg([
    '-y',
    '-ss',
    ss,
    '-to',
    to,
    '-i',
    raw,
    '-i',
    palette,
    '-lavfi',
    `${filters}[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=4`,
    '-loop',
    '0',
    GIF_SITE,
  ]);
  await copyFile(GIF_SITE, GIF_README);
  await rm(scratch, { recursive: true, force: true });

  const mp4 = await ffprobe(MP4_OUT);
  const gif = await ffprobe(GIF_SITE);
  if (mp4.width !== 1440 || mp4.height !== 900) {
    fail(`mp4 is ${mp4.width}x${mp4.height}, expected 1440x900`);
  }
  if (gif.width !== GIF_WIDTH) fail(`gif width is ${gif.width}, expected ${GIF_WIDTH}`);
  const seconds = Number(mp4.duration);
  if (seconds < 50 || seconds > 62) {
    fail(`hero duration ${seconds.toFixed(2)}s is outside 50–62s`);
  }
  const gifBytes = (await stat(GIF_SITE)).size;
  console.log(`hero: mp4 ${mp4.width}x${mp4.height} ${seconds.toFixed(2)}s`);
  console.log(
    `hero: gif ${gif.width}x${gif.height} ${(gifBytes / (1024 * 1024)).toFixed(1)}MB`,
  );
}

async function record(scene) {
  const browser = await launchBrowser();
  const videoDir = await mkdtemp(path.join(tmpdir(), 'coppice-hero-video-'));
  let raw = null;
  let trim = null;
  try {
    const context = await browser.newContext({
      viewport: VIEWPORT,
      deviceScaleFactor: 1,
      colorScheme: 'light',
      locale: 'en-US',
      timezoneId: 'UTC',
      recordVideo: { dir: videoDir, size: VIEWPORT },
    });
    await context.addInitScript((styleText) => {
      localStorage.setItem('coppice.theme', 'light');
      localStorage.setItem('coppice.sidebar.collapsed', '0');
      localStorage.setItem('coppice.plugins.guideOpen', '0');
      const hideInfra = () => {
        for (const el of document.querySelectorAll('dd')) {
          if ((el.textContent || '').includes('/data/')) {
            if (el.parentElement) el.parentElement.style.display = 'none';
          }
        }
      };
      const install = () => {
        const root = document.head || document.documentElement;
        if (!root || !document.body) return;
        if (!document.getElementById('coppice-hero-caption-style')) {
          const style = document.createElement('style');
          style.id = 'coppice-hero-caption-style';
          style.textContent = styleText;
          root.appendChild(style);
        }
        let bar = document.getElementById('coppice-hero-caption');
        if (!bar) {
          bar = document.createElement('div');
          bar.id = 'coppice-hero-caption';
          document.body.appendChild(bar);
        }
        const caption = sessionStorage.getItem('coppice.hero.caption') ?? '';
        bar.textContent = caption;
        bar.style.display = caption ? 'block' : 'none';
        hideInfra();
      };
      const boot = () => {
        install();
        if (!document.body) return;
        new MutationObserver(() => {
          hideInfra();
          if (!document.getElementById('coppice-hero-caption')) install();
        }).observe(document.body, { childList: true, subtree: true });
      };
      if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', boot);
      } else {
        boot();
      }
    }, `${CAPTION_STYLE}
      html,body{overflow:hidden!important}
      *{scrollbar-width:none!important;caret-color:transparent!important}
      ::-webkit-scrollbar{display:none!important}
    `);
    const openedAt = Date.now();
    const page = await context.newPage();
    page.on('pageerror', (error) => {
      console.log(`hero: pageerror ${error.message}`);
    });
    await page.route('**/api/tickets/*/comments', async (route) => {
      if (route.request().method() !== 'GET') {
        await route.continue();
        return;
      }
      const response = await route.fetch();
      const body = await response.json().catch(() => null);
      if (!Array.isArray(body)) {
        await route.fulfill({ response });
        return;
      }
      const filtered = body.filter((comment) => {
        const text = String(comment.body ?? '');
        if (comment.intent === 'system_event') return false;
        if (/Technical Lead|couldn.t start|No PM agent is available/.test(text)) return false;
        return true;
      });
      await route.fulfill({ status: response.status(), json: filtered });
    });

    await page.goto(`${WEB}/boards/${scene.boardId}`, { waitUntil: 'domcontentloaded' });
    await page.getByRole('heading', { name: 'Board', level: 1 }).waitFor({ timeout: 30_000 });
    await page.getByText('Remember the last open settings tab').waitFor();
    await page.getByText('Tighten spacing on the settings form').waitFor();
    await preparePage(page);
    await installCaption(page);
    await assertClean(page, 'board');

    await setCaption(page, CAPTIONS[0]);
    const storyStart = Date.now();
    const until = async (ms) => {
      const wait = storyStart + ms - Date.now();
      if (wait > 0) await sleep(wait);
    };
    await until(700);
    const input = page.locator('[data-add-ticket]');
    await input.click();
    await until(1100);
    await input.pressSequentially(TICKET_TITLE, { delay: 68 });
    await until(4200);
    await input.press('Enter');
    const card = page.getByRole('button', { name: new RegExp(TICKET_TITLE) });
    await card.waitFor({ timeout: 15_000 });
    await assertClean(page, 'scene 1');
    await until(6800);
    const stillWrite = await shoot(page, '01-write-ticket');

    const created = await api('GET', `/api/boards/${scene.boardId}/tickets`, scene.auth);
    const tickets = Array.isArray(created) ? created : (created.items ?? []);
    const ticket = tickets.find((item) => item.title === TICKET_TITLE);
    if (!ticket) fail('typed ticket was not saved');
    await api('PATCH', `/api/tickets/${ticket.id}`, scene.auth, {
      description:
        'Settings should offer a dark mode toggle and follow the same theme preference as the rest of the app.',
      priority: 'high',
      repoId: scene.repoId,
    });
    await api('POST', `/api/tickets/${ticket.id}/assign`, scene.auth, {
      agentId: scene.agentIds.PM,
    });
    await api('PATCH', `/api/tickets/${ticket.id}/status`, scene.auth, {
      status: 'plan_review',
    });
    await attachPlan(ticket.id, scene.agentIds.PM);

    await until(7600);
    await setCaption(page, CAPTIONS[1]);
    await card.click();
    await page.getByRole('button', { name: 'Approve plan' }).waitFor({ timeout: 15_000 });
    await page.getByText('acme-web').first().waitFor();
    await page.getByRole('heading', { name: 'Plan', level: 2 }).scrollIntoViewIfNeeded();
    await page.getByText('Add the Settings toggle').waitFor();
    await assertClean(page, 'scene 2');
    await until(13200);
    await shoot(page, '02-plan');

    await setCaption(page, CAPTIONS[2]);
    await page.getByRole('button', { name: 'Approve plan' }).scrollIntoViewIfNeeded();
    await until(14800);
    await page.getByRole('button', { name: 'Approve plan' }).click();
    const detail = page.locator('[aria-label="Ticket detail"]');
    await detail.locator('header').getByText('Ready', { exact: true }).waitFor({
      timeout: 15_000,
    });
    await page.getByRole('button', { name: 'Close', exact: true }).click();
    const readyColumn = page.getByRole('region', { name: 'Ready' });
    await readyColumn.getByText(TICKET_TITLE).waitFor({ timeout: 15_000 });
    await scrollBoardTo(page, 'Ready');
    await setCaption(page, CAPTIONS[2]);
    await assertClean(page, 'scene 3');
    await until(21200);
    await shoot(page, '03-approve');

    await setCaption(page, CAPTIONS[3]);
    const worktreePromise = heroGit('worktree', ticket.id.split('-')[0]);
    await scrollBoardTo(page, 'In Progress');
    await page.getByText('Cover theme switching with a smoke test').waitFor();
    await page.getByText('Review the settings page at a narrow width').waitFor();
    await assertClean(page, 'scene 4');
    await until(23000);
    await shoot(page, '04-build');

    const short = ticket.id.split('-')[0];
    const worktree = await worktreePromise;
    if (!worktree?.startsWith('/data/worktrees/')) {
      fail(`worktree path missing: ${worktree}`);
    }
    await api('PATCH', `/api/tickets/${ticket.id}/status`, scene.auth, {
      status: 'wait_for_final_review',
    });
    await psql(`
      UPDATE tickets
      SET branch_name = ${sqlQuote(`agent/TICKET-${short}`)}
      WHERE id = ${sqlQuote(ticket.id)}::uuid;
    `);

    await until(28500);
    await setCaption(page, CAPTIONS[4]);
    const review = new URL('/code', WEB);
    review.searchParams.set('repoId', scene.repoId);
    review.searchParams.set('ticketId', ticket.id);
    review.searchParams.set('worktree', worktree);
    review.searchParams.set('baseBranch', 'main');
    await page.goto(review.href, { waitUntil: 'domcontentloaded' });
    await page.getByText('Code review', { exact: true }).waitFor({ timeout: 30_000 });
    await page.getByText('acme-web').first().waitFor();
    await page.getByText('src/settings/ThemeToggle.tsx').waitFor();
    await page.getByRole('button', { name: 'Unified' }).click();
    await page.getByText("useState('light')").first().waitFor({ timeout: 20_000 });
    await preparePage(page);
    await installCaption(page);
    await setCaption(page, CAPTIONS[4]);
    await assertClean(page, 'scene 5 diff');
    await until(33500);
    const line = page.locator('tr').filter({ hasText: "useState('light')" }).first();
    await line.locator('td.diff-gutter').last().click();
    const commentBox = page.getByPlaceholder('Leave a comment…');
    await commentBox.waitFor({ timeout: 10_000 });
    await commentBox.pressSequentially(LINE_COMMENT, { delay: 42 });
    await page.getByRole('button', { name: 'Add comment', exact: true }).click();
    await page.getByText(LINE_COMMENT).waitFor();
    await assertClean(page, 'scene 5 comment');
    await until(42000);
    await shoot(page, '05-diff');

    await setCaption(page, CAPTIONS[5]);
    await page.goto(`${WEB}/boards/${scene.boardId}?ticket=${ticket.id}`, {
      waitUntil: 'domcontentloaded',
    });
    const ticketHeading = page.getByRole('heading', { name: TICKET_TITLE }).first();
    await ticketHeading.waitFor({ timeout: 30_000 });
    await page.getByRole('button', { name: 'Accept', exact: true }).waitFor();
    await preparePage(page);
    await installCaption(page);
    await setCaption(page, CAPTIONS[5]);
    await ticketHeading.click();
    await page.keyboard.press('Control+Shift+Enter');
    await page.getByRole('dialog', { name: 'Accept this ticket?' }).waitFor({ timeout: 10_000 });
    await assertClean(page, 'scene 6');
    await until(47500);
    await shoot(page, '06-accept');

    await page.getByRole('dialog', { name: 'Accept this ticket?' }).getByRole('button', { name: 'Cancel' }).click();
    await page.getByRole('button', { name: 'Close', exact: true }).click();
    await page.getByRole('dialog', { name: 'Accept this ticket?' }).waitFor({ state: 'hidden' });
    await scrollBoardTo(page, 'Backlog');
    await setCaption(page, null);
    await page.getByText('Remember the last open settings tab').waitFor();
    await assertClean(page, 'scene 7');
    // Hold the clean board long enough that the loop reads as a cut back to scene 1.
    // Nudge the pointer so the recorder keeps emitting frames after the dialog closes.
    for (let i = 0; i < 8; i += 1) {
      await page.mouse.move(8 + i * 3, 8);
      await sleep(450);
    }
    const stillBoard = await shoot(page, '07-board');
    await page.mouse.move(20, 12);
    await sleep(800);
    await copyFile(stillBoard, POSTER);
    console.log(`hero: wrote ${POSTER}`);

    const storyEnd = Date.now();
    const video = page.video();
    await page.close();
    await context.close();
    raw = await video.path();
    const rawDuration = Number((await ffprobe(raw)).duration);
    const lead = (storyStart - openedAt) / 1000;
    const storyLength = (storyEnd - storyStart) / 1000;
    trim = {
      // The caption paints just after storyStart. Skip the beat before it.
      start: Math.max(0, lead + 0.55),
      // Recorder timestamps lag wall clock, so the board hold sits in the tail.
      end: Math.min(rawDuration - 0.25, lead + storyLength + 6),
    };
    console.log(
      `hero: raw video ${raw} duration ${rawDuration.toFixed(2)}s wallEnd ${wallEnd.toFixed(2)}s`,
    );
    console.log(`hero: still ${stillWrite}`);
  } finally {
    await browser.close();
  }
  if (!raw || !trim) fail('recording did not finish');
  await encode(raw, trim.start, trim.end);
  await rm(videoDir, { recursive: true, force: true });
}

async function main() {
  console.log(`hero: waiting for ${API}/health and ${WEB}`);
  await waitForOk(`${API}/health`, 'API');
  await waitForOk(WEB, 'web');
  await requireDesktopMode();
  const auth = await desktopSession();
  const seeded = await seedWorkspace(auth);
  await record({ ...seeded, auth });
  for (const caption of CAPTIONS) console.log(`hero: caption ${JSON.stringify(caption)}`);
}

main().catch((err) => {
  fail(err instanceof Error ? err.stack ?? err.message : String(err));
});
