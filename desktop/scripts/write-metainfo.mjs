#!/usr/bin/env node
// Writes the AppStream metainfo that electron-builder installs to
// /usr/share/metainfo. The release version comes from desktop/package.json
// (release-version.mjs updates that file before dist). There is no project
// license field: the owner has not chosen one. metadata_license covers this
// XML file only, which AppStream requires.

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const desktopDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export const METAINFO_PATH = join(desktopDir, 'build/linux/dev.coppice.app.metainfo.xml');

export const SUMMARY = 'Local board for your AI agent team';
export const LONG_DESCRIPTION =
  'Agents plan, you approve. Agents build, you review. Coppice runs on your machine with the agent CLIs you already pay for, like Claude Code and Codex. No account, no Docker.';
export const HOMEPAGE = 'https://getcoppice.vercel.app/';
export const SCREENSHOT = 'https://getcoppice.vercel.app/assets/hero-screenshot.png';
export const SCREENSHOT_CAPTION = 'The Coppice board';

function escapeXml(value) {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

export function releaseDateFor(version, existingXml, today) {
  if (existingXml) {
    const match = existingXml.match(/<release version="([^"]+)" date="([^"]+)"\/>/);
    if (match && match[1] === version) return match[2];
  }
  return today;
}

export function renderMetainfo({ version, date }) {
  return `<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>dev.coppice.app</id>
  <!-- License of this metainfo file, not of Coppice. No project license is set. -->
  <metadata_license>CC0-1.0</metadata_license>
  <name>Coppice</name>
  <summary>${escapeXml(SUMMARY)}</summary>
  <description>
    <p>${escapeXml(LONG_DESCRIPTION)}</p>
  </description>
  <icon type="stock">coppice</icon>
  <url type="homepage">${escapeXml(HOMEPAGE)}</url>
  <developer id="io.github.fertile-org">
    <name>fertile-org</name>
  </developer>
  <launchable type="desktop-id">coppice.desktop</launchable>
  <screenshots>
    <screenshot type="default">
      <caption>${escapeXml(SCREENSHOT_CAPTION)}</caption>
      <image type="source" width="1440" height="900">${escapeXml(SCREENSHOT)}</image>
    </screenshot>
  </screenshots>
  <releases>
    <release version="${escapeXml(version)}" date="${escapeXml(date)}"/>
  </releases>
  <content_rating type="oars-1.1"/>
  <categories>
    <category>Development</category>
  </categories>
</component>
`;
}

export function utcDate(now = new Date()) {
  return now.toISOString().slice(0, 10);
}

export function writeMetainfo(dir = desktopDir, now = new Date()) {
  const pkg = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'));
  const output = join(dir, 'build/linux/dev.coppice.app.metainfo.xml');
  let existing = null;
  try {
    existing = readFileSync(output, 'utf8');
  } catch {
    existing = null;
  }
  const xml = renderMetainfo({
    version: pkg.version,
    date: releaseDateFor(pkg.version, existing, utcDate(now)),
  });
  writeFileSync(output, xml);
  return output;
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  console.log(writeMetainfo());
}
