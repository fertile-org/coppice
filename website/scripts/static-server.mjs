import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.webp': 'image/webp',
  '.gif': 'image/gif',
  '.ico': 'image/x-icon',
  '.webm': 'video/webm',
  '.mp4': 'video/mp4',
  '.vtt': 'text/vtt; charset=utf-8',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
};

function inside(root, candidate) {
  const full = path.resolve(root, candidate);
  return full === root || full.startsWith(root + path.sep) ? full : null;
}

function existingFile(root, candidate) {
  const full = inside(root, candidate);
  if (!full) return null;
  if (fs.existsSync(full) && fs.statSync(full).isFile()) return full;
  return null;
}

/**
 * @param {string} dist
 * @param {string} pathname
 * @param {'clean' | 'directory'} style
 * `clean` matches Vercel cleanUrls + trailingSlash:false (docs.html served at /docs).
 * `directory` matches GitHub Pages / a plain static host (docs/index.html at /docs/).
 */
export function resolveRequest(dist, pathname, style) {
  const root = path.resolve(dist);
  let raw = pathname.split('?')[0];
  try {
    raw = decodeURIComponent(raw);
  } catch {
    return { status: 400 };
  }
  const trailing = raw.length > 1 && raw.endsWith('/');
  const bare = trailing ? raw.slice(0, -1) : raw;
  const rel = bare.replace(/^\/+/, '');

  if (style === 'directory') {
    if (rel === '') {
      const index = existingFile(root, 'index.html');
      return index ? { status: 200, file: index } : { status: 404 };
    }
    const asFile = existingFile(root, rel);
    if (asFile) return { status: 200, file: asFile };
    const index = existingFile(root, path.join(rel, 'index.html'));
    if (index && trailing) return { status: 200, file: index };
    if (index) return { status: 308, location: `${bare}/` };
    return { status: 404 };
  }

  if (trailing) return { status: 308, location: bare || '/' };
  const candidates = rel === '' ? ['index.html'] : [rel, `${rel}.html`, path.join(rel, 'index.html')];
  for (const candidate of candidates) {
    const file = existingFile(root, candidate);
    if (file) return { status: 200, file };
  }
  return { status: 404 };
}

export function startStaticServer(dist, { style = 'clean', host = '127.0.0.1', mount = '' } = {}) {
  const root = path.resolve(dist);
  const prefix = !mount || mount === '/' ? '' : mount.replace(/\/$/, '');
  const server = http.createServer((req, res) => {
    const url = new URL(req.url || '/', 'http://127.0.0.1');
    let pathname = url.pathname;
    if (prefix) {
      if (pathname === prefix) pathname = '/';
      else if (pathname.startsWith(`${prefix}/`)) pathname = pathname.slice(prefix.length) || '/';
      else pathname = '/__outside_mount__';
    }
    const resolved = resolveRequest(root, pathname, style);
    if (resolved.status === 308 && resolved.location) {
      const location = resolved.location.startsWith('/') ? resolved.location : `/${resolved.location}`;
      res.writeHead(308, { Location: `${prefix}${location}` });
      res.end();
      return;
    }
    if (resolved.status === 200 && resolved.file) {
      res.writeHead(200, { 'Content-Type': TYPES[path.extname(resolved.file).toLowerCase()] || 'application/octet-stream' });
      fs.createReadStream(resolved.file).pipe(res);
      return;
    }
    const notFound = existingFile(root, '404.html');
    if (notFound) {
      res.writeHead(404, { 'Content-Type': 'text/html; charset=utf-8' });
      fs.createReadStream(notFound).pipe(res);
      return;
    }
    res.writeHead(resolved.status === 400 ? 400 : 404);
    res.end('not found');
  });

  return new Promise((resolve) => {
    server.listen(0, host, () => {
      const address = server.address();
      const port = typeof address === 'object' && address ? address.port : 0;
      resolve({
        url: `http://${host}:${port}`,
        close: () =>
          new Promise((done, reject) => {
            server.close((error) => (error ? reject(error) : done()));
          }),
      });
    });
  });
}
