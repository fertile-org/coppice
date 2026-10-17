#!/usr/bin/env node
// Writes updates/latest.json for the update host: the newest published
// release, small enough to fetch every few hours from every install.
//
//   node desktop/scripts/latest-json.mjs --tag v0.1.0-rc.8 --repo fertile-org/coppice --out site/updates/latest.json

import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
import { pathToFileURL } from 'node:url';
import { versionFromTag } from '../src/updateCheck.mjs';

const TAG_RE = /^v\d+\.\d+\.\d+(?:-rc\.\d+)?$/;

export function latestJson({ tag, repo }) {
  if (!TAG_RE.test(tag ?? '')) throw new Error(`unsupported release tag ${JSON.stringify(tag)}`);
  if (!/^[\w.-]+\/[\w.-]+$/.test(repo ?? '')) throw new Error(`bad repository ${JSON.stringify(repo)}`);
  return `${JSON.stringify({
    version: versionFromTag(tag),
    tag,
    url: `https://github.com/${repo}/releases/tag/${tag}`,
  })}\n`;
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const { values } = parseArgs({
    options: { tag: { type: 'string' }, repo: { type: 'string' }, out: { type: 'string' } },
  });
  try {
    if (!values.out) throw new Error('--out is required');
    mkdirSync(dirname(values.out), { recursive: true });
    writeFileSync(values.out, latestJson(values));
  } catch (err) {
    console.error(err.message);
    process.exit(1);
  }
}
