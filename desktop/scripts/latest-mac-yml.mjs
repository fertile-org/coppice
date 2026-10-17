#!/usr/bin/env node
// Writes the electron-updater manifest for a signed macOS build.
// File URLs are absolute GitHub release asset URLs so the manifest can live
// on GitHub Pages while the zips stay on the release.

import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { basename } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

function yamlString(value) {
  return `'${String(value).replaceAll("'", "''")}'`;
}

export async function zipMetadata(zipPath, assetBase) {
  const data = await readFile(zipPath);
  const name = basename(zipPath);
  const base = assetBase.replace(/\/$/, '');
  return {
    url: `${base}/${name}`,
    sha512: createHash('sha512').update(data).digest('base64'),
    size: data.length,
  };
}

export function renderLatestMacYml({ version, files, releaseDate }) {
  const lines = [`version: ${version}`, 'files:'];
  for (const file of files) {
    lines.push(`  - url: ${yamlString(file.url)}`);
    lines.push(`    sha512: ${yamlString(file.sha512)}`);
    lines.push(`    size: ${file.size}`);
  }
  const primary = files.find((file) => file.url.includes('arm64')) ?? files[0];
  lines.push(`path: ${yamlString(primary.url)}`);
  lines.push(`sha512: ${yamlString(primary.sha512)}`);
  lines.push(`releaseDate: ${yamlString(releaseDate)}`);
  return `${lines.join('\n')}\n`;
}

async function main() {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: {
      version: { type: 'string' },
      'asset-base': { type: 'string' },
      out: { type: 'string' },
      date: { type: 'string' },
    },
  });
  if (!values.version || !values['asset-base'] || !values.out || positionals.length === 0) {
    console.error('usage: latest-mac-yml.mjs --version V --asset-base URL --out latest-mac.yml <zip>...');
    process.exit(2);
  }
  const files = [];
  for (const zip of positionals) {
    files.push(await zipMetadata(zip, values['asset-base']));
  }
  const yml = renderLatestMacYml({
    version: values.version,
    files,
    releaseDate: values.date ?? new Date().toISOString(),
  });
  await writeFile(values.out, yml);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
