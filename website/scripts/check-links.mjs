import fs from 'node:fs';
import path from 'node:path';
import { startStaticServer } from './static-server.mjs';

function flag(name, fallback) {
  const inline = process.argv.find((arg) => arg.startsWith(`${name}=`));
  if (inline) return inline.slice(name.length + 1);
  const index = process.argv.indexOf(name);
  if (index >= 0 && process.argv[index + 1] && !process.argv[index + 1].startsWith('--')) return process.argv[index + 1];
  return fallback;
}

const dist = path.resolve(flag('--dist', 'dist'));
const style = flag('--host-style', 'clean');

function normalizeMount(value) {
  if (!value || value === '/') return '';
  let mount = String(value).trim();
  if (!mount.startsWith('/')) mount = `/${mount}`;
  if (mount.length > 1 && mount.endsWith('/')) mount = mount.slice(0, -1);
  return mount;
}

const mount = normalizeMount(process.env.BASE_PATH ?? process.env.SITE_BASE);

if (!fs.existsSync(dist)) {
  console.error(`No build output at ${dist}. Run npm run build first.`);
  process.exit(1);
}

const ATTR = /(?:href|src|poster)\s*=\s*(?:"([^"]*)"|'([^']*)')/gi;
const CSS_URL = /url\(\s*(?:"([^"]+)"|'([^']+)'|([^)"'\s]+))\s*\)/g;

function htmlFiles(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) htmlFiles(full, out);
    else if (entry.name.endsWith('.html')) out.push(full);
  }
  return out;
}

function fileToPath(file) {
  let rel = path.relative(dist, file).split(path.sep).join('/');
  if (rel === '404.html') return null;
  if (rel.endsWith('/index.html')) rel = rel.slice(0, -'/index.html'.length);
  else if (rel.endsWith('.html')) rel = rel.slice(0, -'.html'.length);
  if (rel === 'index' || rel === '') return mount || '/';
  const pathName = `/${rel}`;
  return mount ? `${mount}${pathName}` : pathName;
}

function canonicalPath(pathname) {
  if (!pathname) return '/';
  if (pathname.length > 1 && pathname.endsWith('/')) return pathname.slice(0, -1);
  return pathname;
}

function idsIn(html) {
  const ids = new Set();
  const re = /\sid\s*=\s*(?:"([^"]+)"|'([^']+)')/gi;
  let match;
  while ((match = re.exec(html))) ids.add(match[1] || match[2]);
  return ids;
}

function refsIn(text, kind) {
  const refs = [];
  const re = kind === 'css' ? CSS_URL : ATTR;
  re.lastIndex = 0;
  let match;
  while ((match = re.exec(text))) {
    const value = (match[1] || match[2] || match[3] || '').trim();
    if (!value || value.startsWith('data:') || value.startsWith('mailto:') || value.startsWith('javascript:')) continue;
    refs.push(value);
  }
  return refs;
}

const server = await startStaticServer(dist, { style, mount });
const origin = new URL(server.url).origin;
const failures = [];
const checked = new Set();
const pages = new Map();
const fragments = [];

function classify(ref, baseHref) {
  let target;
  try {
    target = new URL(ref, baseHref);
  } catch {
    failures.push(`${baseHref} has unparseable link ${ref}`);
    return null;
  }
  if (target.origin !== origin) return null;
  return target;
}

async function fetchPath(pathname) {
  const response = await fetch(new URL(pathname, server.url), { redirect: 'follow' });
  const body = Buffer.from(await response.arrayBuffer());
  return { response, finalUrl: new URL(response.url), body };
}

try {
  const queue = [mount || '/'];
  for (const file of htmlFiles(dist)) {
    const urlPath = fileToPath(file);
    if (urlPath) queue.push(urlPath);
  }

  while (queue.length) {
    const urlPath = canonicalPath(queue.shift());
    if (checked.has(urlPath)) continue;
    checked.add(urlPath);

    let result;
    try {
      result = await fetchPath(urlPath);
    } catch (error) {
      failures.push(`${urlPath} failed to fetch (${error.message})`);
      continue;
    }
    const { response, finalUrl, body } = result;
    if (finalUrl.origin !== origin) {
      failures.push(`${urlPath} redirected off-origin to ${finalUrl.href}`);
      continue;
    }
    if (!response.ok) {
      failures.push(`${urlPath} -> ${response.status} ${finalUrl.pathname}`);
      continue;
    }

    const type = response.headers.get('content-type') || '';
    if (type.includes('text/html')) {
      const text = body.toString('utf8');
      if (text.includes('<h1>Page not found.</h1>')) {
        failures.push(`${urlPath} served the site 404 page`);
        continue;
      }
      pages.set(canonicalPath(finalUrl.pathname), text);
      const baseHref = finalUrl.href;
      for (const ref of refsIn(text, 'html')) {
        const target = classify(ref, baseHref);
        if (!target) continue;
        if (target.hash.length > 1) fragments.push({ from: urlPath, ref, target });
        queue.push(target.pathname);
      }
    } else if (type.includes('text/css')) {
      const text = body.toString('utf8');
      for (const ref of refsIn(text, 'css')) {
        const target = classify(ref, finalUrl.href);
        if (target) queue.push(target.pathname);
      }
    }
  }

  for (const { from, ref, target } of fragments) {
    const hash = decodeURIComponent(target.hash.slice(1));
    const owner = pages.get(canonicalPath(target.pathname));
    if (!owner) {
      failures.push(`${from} fragment ${ref} has no page ${target.pathname}`);
      continue;
    }
    if (!idsIn(owner).has(hash)) failures.push(`${from} -> ${ref} missing #${hash}`);
  }
} finally {
  await server.close();
}

const internal = [...checked].sort();
if (failures.length) {
  console.error(`Link check failed (${style}, ${internal.length} URLs):`);
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}

console.log(`Link check passed (${style}${mount ? `, mount ${mount}` : ''}, ${internal.length} URLs).`);
for (const item of internal) console.log(`  ${item}`);
