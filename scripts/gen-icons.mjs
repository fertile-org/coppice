#!/usr/bin/env node
// Renders every committed Coppice icon from assets/brand/coppice-logo.svg.
//
//   make gen-icons
//
// The SVG is a trace of the old 1254px raster, not an official designer file.
// Drop in a replacement SVG at the same path and run this again.
//
// Linux icons are transparent. macOS (desktop/build/icon.png) and the Apple
// touch icon sit on the paper-colored rounded square the Dock and home screen
// expect, with padding so the tree is not clipped by the corner mask.

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Resvg } from '@resvg/resvg-js';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const sourcePath = join(root, 'assets/brand/coppice-logo.svg');

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
  writeFileSync(join(root, 'website/public/favicon.svg'), source);
  console.log('rendered icons from assets/brand/coppice-logo.svg');
}

main();
