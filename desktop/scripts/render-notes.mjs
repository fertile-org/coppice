#!/usr/bin/env node
// Renders the release install notes. Lines between `{{MAC_UNSIGNED}}` and
// `{{/MAC_UNSIGNED}}` (each marker on its own line) are kept, markers stripped,
// when the macOS build is unsigned; otherwise the block is dropped. Runs of
// blank lines are collapsed to one.
//
//   node scripts/render-notes.mjs --template <file> --out <file> --mac-unsigned <true|false>
//   node scripts/render-notes.mjs --template <file> --out <file> --mac-unsigned <true|false> --splice-into <body>

import { readFile, writeFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

const OPEN = '{{MAC_UNSIGNED}}';
const CLOSE = '{{/MAC_UNSIGNED}}';

// Keeps the generated changelog and replaces an existing "## Install" section,
// or appends one. A later run with the other signed/unsigned choice overwrites
// that section instead of stacking a second copy.
export function spliceInstallNotes(body, installSection) {
  const section = String(installSection ?? '').replace(/\s+$/, '');
  const text = String(body ?? '').replace(/\r\n/g, '\n');
  const lines = text.split('\n');
  const start = lines.findIndex((line) => line.trim() === '## Install');
  const head = (start === -1 ? text : lines.slice(0, start).join('\n')).replace(/\s+$/, '');
  return head ? `${head}\n\n${section}\n` : `${section}\n`;
}

export function renderNotes(template, { macUnsigned }) {
  const out = [];
  let inBlock = false;
  const lines = template.split('\n');
  lines.forEach((line, i) => {
    const marker = line.trim();
    if (marker === OPEN) {
      if (inBlock) throw new Error(`nested ${OPEN} at line ${i + 1}`);
      inBlock = true;
    } else if (marker === CLOSE) {
      if (!inBlock) throw new Error(`${CLOSE} without ${OPEN} at line ${i + 1}`);
      inBlock = false;
    } else if (!inBlock || macUnsigned) {
      out.push(line);
    }
  });
  if (inBlock) throw new Error(`unclosed ${OPEN}`);
  return out.join('\n').replace(/\n{3,}/g, '\n\n');
}

async function main() {
  const { values } = parseArgs({
    options: {
      template: { type: 'string' },
      out: { type: 'string' },
      'mac-unsigned': { type: 'string' },
      'splice-into': { type: 'string' },
    },
  });
  const flag = values['mac-unsigned'];
  if (!values.template || !values.out || (flag !== 'true' && flag !== 'false')) {
    console.error(
      'usage: render-notes.mjs --template <file> --out <file> --mac-unsigned <true|false> [--splice-into <body>]',
    );
    process.exit(2);
  }
  const template = await readFile(values.template, 'utf8');
  const section = renderNotes(template, { macUnsigned: flag === 'true' });
  const notes = values['splice-into']
    ? spliceInstallNotes(await readFile(values['splice-into'], 'utf8'), section)
    : section;
  await writeFile(values.out, notes);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
