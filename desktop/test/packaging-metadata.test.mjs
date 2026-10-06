import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, it } from 'node:test';

import {
  HOMEPAGE,
  LONG_DESCRIPTION,
  SCREENSHOT,
  SUMMARY,
  releaseDateFor,
  renderMetainfo,
} from '../scripts/write-metainfo.mjs';

const desktopDir = join(dirname(fileURLToPath(import.meta.url)), '..');

function read(rel) {
  return readFileSync(join(desktopDir, rel), 'utf8');
}

function assertAscii(label, value) {
  assert.ok(/^[\n\r\t\x20-\x7e]*$/.test(value), `${label} must be plain ASCII`);
}

describe('desktop package copy', () => {
  const pkg = JSON.parse(read('package.json'));
  const yml = read('electron-builder.yml');

  it('keeps the dpkg package name coppice-desktop and the display name Coppice', () => {
    assert.equal(pkg.name, 'coppice-desktop');
    assert.equal(pkg.productName, 'Coppice');
    assert.match(yml, /^ {2}packageName: coppice-desktop$/m);
    assert.match(yml, /^productName: Coppice$/m);
  });

  it('uses the short summary for the Debian synopsis and the long copy for the description', () => {
    assert.equal(pkg.description, LONG_DESCRIPTION);
    assert.match(yml, new RegExp(`^  synopsis: ${SUMMARY}$`, 'm'));
    assertAscii('summary', SUMMARY);
    assertAscii('description', LONG_DESCRIPTION);
    assertAscii('homepage', HOMEPAGE);
    assert.equal(pkg.homepage, HOMEPAGE);
  });

  it('points StartupWMClass at the Electron window class and ships metainfo', () => {
    assert.match(yml, /^ {6}StartupWMClass: coppice$/m);
    assert.match(
      yml,
      /dev\.coppice\.app\.metainfo\.xml=\/usr\/share\/metainfo\/dev\.coppice\.app\.metainfo\.xml/,
    );
    const windows = read('src/windows.mjs');
    assert.match(windows, /path\.join\(STATIC_DIR, 'icon\.png'\)/);
    assert.doesNotMatch(windows, /!app\.isPackaged/);
  });
});

describe('AppStream metainfo', () => {
  const xml = renderMetainfo({ version: '0.1.0-rc.2', date: '2026-10-06' });

  it('carries the store name, copy, homepage, and hero screenshot', () => {
    assert.match(xml, /<name>Coppice<\/name>/);
    assert.match(xml, /<summary>Local board for your AI agent team<\/summary>/);
    assert.match(xml, /<p>Agents plan, you approve\./);
    assert.match(xml, /<url type="homepage">https:\/\/getcoppice\.vercel\.app\/<\/url>/);
    assert.match(xml, /width="1440" height="900">https:\/\/getcoppice\.vercel\.app\/assets\/hero-screenshot\.png/);
    assert.match(xml, /<caption>The Coppice board<\/caption>/);
    assert.match(xml, /<launchable type="desktop-id">coppice\.desktop<\/launchable>/);
    assert.match(xml, /<name>fertile-org<\/name>/);
    assert.match(xml, /<release version="0\.1\.0-rc\.2" date="2026-10-06"\/>/);
    assert.doesNotMatch(xml, /project_license/);
    assertAscii('metainfo', xml);
  });

  it('keeps the release date when the version is unchanged', () => {
    const existing = '<release version="1.2.3" date="2020-01-02"/>';
    assert.equal(releaseDateFor('1.2.3', existing, '2026-10-06'), '2020-01-02');
    assert.equal(releaseDateFor('1.2.4', existing, '2026-10-06'), '2026-10-06');
    assert.equal(SCREENSHOT, 'https://getcoppice.vercel.app/assets/hero-screenshot.png');
  });
});
