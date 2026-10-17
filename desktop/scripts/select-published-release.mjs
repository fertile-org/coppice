#!/usr/bin/env node
// Picks the release whose apt repo and macOS update manifest should be published.
// Drafts are ignored. Prereleases compete with final releases by version.

import { spawnSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { isNewer, versionFromTag } from '../src/updateCheck.mjs';

const TAG_RE = /^v\d+\.\d+\.\d+(?:-rc\.\d+)?$/;

export function selectPublishedRelease(releases) {
  let best = null;
  for (const release of releases ?? []) {
    if (!release) continue;
    if (release.draft === true || release.isDraft === true) continue;
    const tag = release.tag_name ?? release.tagName;
    if (typeof tag !== 'string' || !TAG_RE.test(tag)) continue;
    if (!best || isNewer(tag, best.tag)) best = { tag, version: versionFromTag(tag) };
  }
  return best;
}

function writeOutput(chosen) {
  const line = `tag=${chosen.tag}\nversion=${chosen.version}\n`;
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, line);
  process.stdout.write(line);
}

function fail(message) {
  console.error(message);
  process.exit(1);
}

async function main() {
  if (process.env.EVENT_NAME === 'release') {
    if (process.env.EVENT_DRAFT === 'true') {
      fail('Refusing to publish update metadata for a draft release.');
    }
    const chosen = selectPublishedRelease([{ tag_name: process.env.EVENT_TAG, draft: false }]);
    if (!chosen) fail(`unsupported release tag ${JSON.stringify(process.env.EVENT_TAG)}`);
    writeOutput(chosen);
    return;
  }

  const listed = spawnSync(
    'gh',
    ['release', 'list', '--limit', '100', '--json', 'tagName,isDraft'],
    { encoding: 'utf8' },
  );
  if (listed.status !== 0) fail(listed.stderr || 'gh release list failed');
  const chosen = selectPublishedRelease(JSON.parse(listed.stdout));
  if (!chosen) fail('No published release to mirror.');
  writeOutput(chosen);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
