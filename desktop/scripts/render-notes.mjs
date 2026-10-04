#!/usr/bin/env node
// Renders the release install notes. Lines between `{{MAC_UNSIGNED}}` and
// `{{/MAC_UNSIGNED}}` (each marker on its own line) are kept, markers stripped,
// when the macOS build is unsigned; otherwise the block is dropped.
//
//   node scripts/render-notes.mjs --template <file> --out <file> --mac-unsigned <true|false>

import { readFile, writeFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

const OPEN = '{{MAC_UNSIGNED}}';
const CLOSE = '{{/MAC_UNSIGNED}}';

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
  return out.join('\n');
}

async function main() {
  const { values } = parseArgs({
    options: {
      template: { type: 'string' },
      out: { type: 'string' },
      'mac-unsigned': { type: 'string' },
    },
  });
  const flag = values['mac-unsigned'];
  if (!values.template || !values.out || (flag !== 'true' && flag !== 'false')) {
    console.error('usage: render-notes.mjs --template <file> --out <file> --mac-unsigned <true|false>');
    process.exit(2);
  }
  const template = await readFile(values.template, 'utf8');
  await writeFile(values.out, renderNotes(template, { macUnsigned: flag === 'true' }));
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
