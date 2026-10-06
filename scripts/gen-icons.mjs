#!/usr/bin/env node
// Renders app and header icons from assets/brand/coppice-logo.svg (the full tree).
// The marketing-site favicon is Hung's circuit-stump mark: the 100×100 RGB
// source in coppice-favicon-source.png, with hand-tuned 16 and 32 rasters.
// A straight downsample turns the circuit roots grey, so those two sizes are
// authored pixels, not a Lanczos shrink. 48 is a coverage scale that keeps
// ink instead of blending it into the cream.
//
//   make gen-icons
//
// The tree SVG is a trace of the old 1254px raster, not an official designer file.
// Drop in a replacement SVG at the same path and run this again.
//
// Linux icons are transparent. macOS (desktop/build/icon.png) and the Apple
// touch icon sit on the paper-colored rounded square the Dock and home screen
// expect, with padding so the tree is not clipped by the corner mask.

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { deflateSync, inflateSync } from 'node:zlib';
import { Resvg } from '@resvg/resvg-js';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const sourcePath = join(root, 'assets/brand/coppice-logo.svg');
const faviconPngPath = join(root, 'assets/brand/coppice-favicon-source.png');
const favicon16Path = join(root, 'assets/brand/coppice-favicon-16.png');
const favicon32Path = join(root, 'assets/brand/coppice-favicon-32.png');
const faviconSvgPath = join(root, 'assets/brand/coppice-favicon.svg');

// Apple's macOS app-icon mask is a squircle of about this corner radius.
const MAC_CORNER_RATIO = 0.2237;
const PAPER = '#faf7f2';
const LINUX_PADDING = 0.04;
const MAC_PADDING = 0.1;

const LINUX_SIZES = [16, 32, 48, 64, 128, 256, 512];

function svgParts(source) {
  const tag = source.match(/<svg\b[^>]*>/);
  if (!tag) throw new Error(`${sourcePath} has no <svg> root`);
  const viewBox = tag[0].match(/viewBox="([^"]+)"/);
  const width = tag[0].match(/\bwidth="([\d.]+)"/);
  const height = tag[0].match(/\bheight="([\d.]+)"/);
  if (!viewBox && !(width && height)) {
    throw new Error(`${sourcePath} needs a viewBox or width and height`);
  }
  const box = viewBox ? viewBox[1] : `0 0 ${width[1]} ${height[1]}`;
  const start = source.indexOf(tag[0]) + tag[0].length;
  const end = source.lastIndexOf('</svg>');
  if (end < start) throw new Error(`${sourcePath} has no closing </svg>`);
  return { viewBox: box, inner: source.slice(start, end) };
}

function canvasSvg(inner, viewBox, size, { padding, background, radius }) {
  const pad = Math.round(size * padding);
  const innerSize = size - pad * 2;
  const backdrop = background
    ? `<rect width="${size}" height="${size}" rx="${radius}" ry="${radius}" fill="${background}"/>`
    : '';
  return `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">
  ${backdrop}
  <svg x="${pad}" y="${pad}" width="${innerSize}" height="${innerSize}" viewBox="${viewBox}" preserveAspectRatio="xMidYMid meet">
    ${inner}
  </svg>
</svg>`;
}

function renderPng(svg, size) {
  const png = new Resvg(svg, {
    fitTo: { mode: 'width', value: size },
    background: 'rgba(0, 0, 0, 0)',
  }).render().asPng();
  return png;
}

function writePng(file, png) {
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, png);
}

// PNG-compressed ICO. Widths above 255 are stored as 0 per the ICO spec.
function pngsToIco(pngs) {
  const count = pngs.length;
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(count, 4);
  let offset = 6 + 16 * count;
  const entries = [];
  for (const png of pngs) {
    const width = png.readUInt32BE(16);
    const height = png.readUInt32BE(20);
    const entry = Buffer.alloc(16);
    entry.writeUInt8(width >= 256 ? 0 : width, 0);
    entry.writeUInt8(height >= 256 ? 0 : height, 1);
    entry.writeUInt16LE(1, 4);
    entry.writeUInt16LE(32, 6);
    entry.writeUInt32LE(png.length, 8);
    entry.writeUInt32LE(offset, 12);
    entries.push(entry);
    offset += png.length;
  }
  return Buffer.concat([header, ...entries, ...pngs]);
}

function paeth(a, b, c) {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  if (pa <= pb && pa <= pc) return a;
  if (pb <= pc) return b;
  return c;
}

function decodePng(buf) {
  if (buf[0] !== 0x89 || buf.toString('ascii', 1, 4) !== 'PNG') {
    throw new Error('not a png');
  }
  let offset = 8;
  let width = 0;
  let height = 0;
  let bitDepth = 0;
  let colorType = 0;
  const idat = [];
  while (offset < buf.length) {
    const length = buf.readUInt32BE(offset);
    offset += 4;
    const type = buf.toString('ascii', offset, offset + 4);
    offset += 4;
    const data = buf.subarray(offset, offset + length);
    offset += length + 4;
    if (type === 'IHDR') {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8];
      colorType = data[9];
    } else if (type === 'IDAT') {
      idat.push(data);
    } else if (type === 'IEND') {
      break;
    }
  }
  if (bitDepth !== 8 || (colorType !== 2 && colorType !== 6)) {
    throw new Error(`unsupported png bitDepth=${bitDepth} colorType=${colorType}`);
  }
  const bpp = colorType === 6 ? 4 : 3;
  const raw = inflateSync(Buffer.concat(idat));
  const stride = width * bpp;
  const rgb = Buffer.alloc(width * height * 3);
  let prev = Buffer.alloc(stride);
  let i = 0;
  for (let y = 0; y < height; y++) {
    const filter = raw[i];
    i += 1;
    const recon = Buffer.alloc(stride);
    for (let x = 0; x < stride; x++) {
      const left = x >= bpp ? recon[x - bpp] : 0;
      const up = prev[x];
      const ul = x >= bpp ? prev[x - bpp] : 0;
      const v = raw[i + x];
      if (filter === 0) recon[x] = v;
      else if (filter === 1) recon[x] = (v + left) & 255;
      else if (filter === 2) recon[x] = (v + up) & 255;
      else if (filter === 3) recon[x] = (v + Math.floor((left + up) / 2)) & 255;
      else if (filter === 4) recon[x] = (v + paeth(left, up, ul)) & 255;
      else throw new Error(`bad png filter ${filter}`);
    }
    i += stride;
    for (let x = 0; x < width; x++) {
      const d = (y * width + x) * 3;
      rgb[d] = recon[x * bpp];
      rgb[d + 1] = recon[x * bpp + 1];
      rgb[d + 2] = recon[x * bpp + 2];
    }
    prev = recon;
  }
  return { width, height, rgb };
}

function pngChunk(tag, data) {
  const type = Buffer.from(tag);
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length, 0);
  const crc = Buffer.alloc(4);
  // node zlib.crc32 exists on recent node; fall back to a small table if not.
  const sum = zlibCrc32(Buffer.concat([type, data]));
  crc.writeUInt32BE(sum >>> 0, 0);
  return Buffer.concat([length, type, data, crc]);
}

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

function zlibCrc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 255] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function encodePng(rgb, width, height) {
  const stride = width * 3;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y++) {
    raw[y * (stride + 1)] = 0;
    rgb.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  const png = Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk('IHDR', ihdr),
    pngChunk('IDAT', deflateSync(raw)),
    pngChunk('IEND', Buffer.alloc(0)),
  ]);
  return png;
}

function scaleNearest(src, dw, dh) {
  const { rgb, width: sw, height: sh } = src;
  const out = Buffer.alloc(dw * dh * 3);
  for (let y = 0; y < dh; y++) {
    const sy = Math.min(sh - 1, Math.floor((y * sh) / dh));
    for (let x = 0; x < dw; x++) {
      const sx = Math.min(sw - 1, Math.floor((x * sw) / dw));
      const s = (sy * sw + sx) * 3;
      const d = (y * dw + x) * 3;
      out[d] = rgb[s];
      out[d + 1] = rgb[s + 1];
      out[d + 2] = rgb[s + 2];
    }
  }
  return out;
}

// Keep a source pixel's color when it covers enough of the output cell.
// Averaging a 2px root with cream is what turns the traces grey at small sizes.
function scalePreserveInk(src, dw, dh) {
  const { rgb, width: sw, height: sh } = src;
  const out = Buffer.alloc(dw * dh * 3);
  for (let y = 0; y < dh; y++) {
    const y0 = Math.floor((y * sh) / dh);
    const y1 = Math.max(y0 + 1, Math.floor(((y + 1) * sh) / dh));
    for (let x = 0; x < dw; x++) {
      const x0 = Math.floor((x * sw) / dw);
      const x1 = Math.max(x0 + 1, Math.floor(((x + 1) * sw) / dw));
      const counts = new Map();
      let ink = 0;
      let total = 0;
      for (let sy = y0; sy < y1 && sy < sh; sy++) {
        for (let sx = x0; sx < x1 && sx < sw; sx++) {
          const i = (sy * sw + sx) * 3;
          const r = rgb[i];
          const g = rgb[i + 1];
          const b = rgb[i + 2];
          total += 1;
          if (Math.abs(r - 246) + Math.abs(g - 241) + Math.abs(b - 230) > 40) {
            ink += 1;
            const key = (r << 16) | (g << 8) | b;
            counts.set(key, (counts.get(key) || 0) + 1);
          }
        }
      }
      const d = (y * dw + x) * 3;
      if (total > 0 && ink * 6 >= total) {
        let best = 0;
        let bestN = -1;
        for (const [key, n] of counts) {
          if (n > bestN) {
            best = key;
            bestN = n;
          }
        }
        out[d] = (best >> 16) & 255;
        out[d + 1] = (best >> 8) & 255;
        out[d + 2] = best & 255;
      } else {
        out[d] = 246;
        out[d + 1] = 241;
        out[d + 2] = 230;
      }
    }
  }
  return out;
}

function hexByte(n) {
  return n.toString(16).padStart(2, '0').toUpperCase();
}

// Crisp 16×16 pixel SVG. Browsers prefer the SVG link over the .ico, and a
// smoothed 100px embed would grey the roots again.
function pixelsToSvg(bitmap) {
  const { rgb, width, height } = bitmap;
  const groups = new Map();
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const i = (y * width + x) * 3;
      const fill = `#${hexByte(rgb[i])}${hexByte(rgb[i + 1])}${hexByte(rgb[i + 2])}`;
      if (!groups.has(fill)) groups.set(fill, []);
      groups.get(fill).push(`<rect x="${x}" y="${y}" width="1" height="1"/>`);
    }
  }
  const body = [...groups.entries()]
    .map(([fill, rects]) => `  <g fill="${fill}">\n    ${rects.join('\n    ')}\n  </g>`)
    .join('\n');
  return `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" shape-rendering="crispEdges">
  <!-- Hand-pixeled circuit-stump favicon. Full mark: assets/brand/coppice-favicon-source.png -->
${body}
</svg>
`;
}

function renderWebsiteFavicon() {
  const source = decodePng(readFileSync(faviconPngPath));
  const hand16 = decodePng(readFileSync(favicon16Path));
  const hand32 = decodePng(readFileSync(favicon32Path));
  if (source.width !== 100 || source.height !== 100) {
    throw new Error(`favicon source must be 100×100, got ${source.width}×${source.height}`);
  }
  if (hand16.width !== 16 || hand16.height !== 16) {
    throw new Error('coppice-favicon-16.png must be 16×16');
  }
  if (hand32.width !== 32 || hand32.height !== 32) {
    throw new Error('coppice-favicon-32.png must be 32×32');
  }
  const png16 = readFileSync(favicon16Path);
  const png32 = readFileSync(favicon32Path);
  const png48 = encodePng(scalePreserveInk(source, 48, 48), 48, 48);
  const apple = encodePng(scaleNearest(source, 180, 180), 180, 180);
  const svg = pixelsToSvg(hand16);
  const website = join(root, 'website/public');
  writeFileSync(faviconSvgPath, svg);
  writeFileSync(join(website, 'favicon.svg'), svg);
  writeFileSync(join(website, 'favicon-32x32.png'), png32);
  writeFileSync(join(website, 'favicon.ico'), pngsToIco([png16, png32, png48]));
  writeFileSync(join(website, 'apple-touch-icon.png'), apple);
}

function main() {
  const source = readFileSync(sourcePath, 'utf8');
  const { viewBox, inner } = svgParts(source);
  const linux = (size) =>
    canvasSvg(inner, viewBox, size, { padding: LINUX_PADDING, background: null, radius: 0 });
  const framed = (size) =>
    canvasSvg(inner, viewBox, size, {
      padding: MAC_PADDING,
      background: PAPER,
      radius: Math.round(size * MAC_CORNER_RATIO),
    });

  writePng(join(root, 'desktop/build/icon.png'), renderPng(framed(1024), 1024));
  writePng(join(root, 'desktop/static/icon.png'), renderPng(linux(1024), 1024));
  for (const size of LINUX_SIZES) {
    writePng(
      join(root, 'desktop/build/icons', `${size}x${size}.png`),
      renderPng(linux(size), size),
    );
  }

  const favicon16 = renderPng(linux(16), 16);
  const favicon32 = renderPng(linux(32), 32);
  writePng(join(root, 'web/public/favicon-16.png'), favicon16);
  writePng(join(root, 'web/public/favicon-32.png'), favicon32);
  writeFileSync(join(root, 'web/public/favicon.ico'), pngsToIco([favicon16, favicon32]));
  writePng(join(root, 'web/public/apple-touch-icon.png'), renderPng(framed(180), 180));
  writeFileSync(join(root, 'web/public/favicon.svg'), source);
  writeFileSync(join(root, 'website/public/logo.svg'), source);
  renderWebsiteFavicon();

  console.log('rendered icons from assets/brand/coppice-logo.svg');
  console.log('rendered website favicon from assets/brand/coppice-favicon-source.png');
}

main();
