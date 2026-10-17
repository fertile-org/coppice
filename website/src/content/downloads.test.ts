import assert from 'node:assert/strict';
import fs from 'node:fs';
import { describe, it } from 'node:test';

import { detectOs, downloadPage, orderOs, type OsId } from './downloads.ts';

const asset = (file: string) =>
  `https://github.com/fertile-org/coppice/releases/latest/download/${file}`;

const macHelp =
  'Not sure which Mac you have? Open Apple menu → About This Mac. If it lists a Chip, choose Apple Silicon. If it lists a Processor, choose Intel.';

describe('download page copy', () => {
  it('keeps the locked strings in one module', () => {
    assert.equal(downloadPage.title, 'Download Coppice');
    assert.equal(downloadPage.intro, 'Free, runs on your own machine, with no Coppice account.');

    assert.deepEqual(
      downloadPage.groups.map((group) => ({
        os: group.os,
        note: group.note ?? null,
        rows: group.rows.map((row) => ({
          name: row.name,
          label: row.action.label,
          href: row.action.href,
          kind: row.action.kind,
        })),
      })),
      [
        {
          os: 'mac',
          note: macHelp,
          rows: [
            {
              name: 'macOS (Apple Silicon)',
              label: 'Download .dmg',
              href: asset('Coppice-mac-arm64.dmg'),
              kind: 'button',
            },
            {
              name: 'macOS (Intel)',
              label: 'Download .dmg',
              href: asset('Coppice-mac-x64.dmg'),
              kind: 'button',
            },
          ],
        },
        {
          os: 'linux',
          note: null,
          rows: [
            {
              name: 'Linux (x64)',
              label: 'Download .deb',
              href: asset('Coppice-linux-amd64.deb'),
              kind: 'button',
            },
            {
              name: 'Linux (ARM64)',
              label: 'Download .deb',
              href: asset('Coppice-linux-arm64.deb'),
              kind: 'button',
            },
          ],
        },
        {
          os: 'windows',
          note: null,
          rows: [
            {
              name: 'Windows 11',
              label: 'Install through WSL',
              href: '/docs/install#windows-11-through-wsl',
              kind: 'link',
            },
          ],
        },
      ],
    );

    assert.equal(
      `${downloadPage.checksum.before}${downloadPage.checksum.sumsLabel}${downloadPage.checksum.between}${downloadPage.checksum.installLabel}${downloadPage.checksum.after}`,
      'Check your download against SHA256SUMS. See Install for how.',
    );
    assert.equal(downloadPage.checksum.sumsHref, asset('SHA256SUMS'));
    assert.equal(downloadPage.checksum.installHref, '/docs/install');

    assert.equal(
      `${downloadPage.next.before}${downloadPage.next.label}${downloadPage.next.after}`,
      'Next, follow the install steps.',
    );
    assert.equal(downloadPage.next.href, '/docs/install');

    const blob = JSON.stringify(downloadPage);
    assert.equal(blob.toLowerCase().includes('mock'), false);
    assert.equal(blob.toLowerCase().includes('fake'), false);
  });
});

describe('download row order', () => {
  it('keeps the no-JS order when the OS is unknown', () => {
    assert.deepEqual(orderOs(null), ['mac', 'linux', 'windows']);
  });

  it('puts both Mac rows first, Apple Silicon then Intel, without reading CPU architecture', () => {
    assert.deepEqual(orderOs('mac'), ['mac', 'linux', 'windows']);
    assert.deepEqual(downloadPage.groups.find((group) => group.os === 'mac')?.rows.map((row) => row.name), [
      'macOS (Apple Silicon)',
      'macOS (Intel)',
    ]);
  });

  it('puts Linux x64 before Linux ARM64, then the other rows in no-JS order', () => {
    assert.deepEqual(orderOs('linux'), ['linux', 'mac', 'windows']);
    assert.deepEqual(downloadPage.groups.find((group) => group.os === 'linux')?.rows.map((row) => row.name), [
      'Linux (x64)',
      'Linux (ARM64)',
    ]);
  });

  it('puts another system first and leaves the rest in no-JS order', () => {
    assert.deepEqual(orderOs('windows'), ['windows', 'mac', 'linux']);
  });
});

describe('detectOs', () => {
  it('detects the OS and ignores CPU architecture', () => {
    const cases: Array<[string, OsId | null]> = [
      [
        'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15',
        'mac',
      ],
      [
        'Mozilla/5.0 (Macintosh; ARM Mac OS X 14_0) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15',
        'mac',
      ],
      ['Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/120.0.0.0 Safari/537.36', 'linux'],
      ['Mozilla/5.0 (X11; Linux aarch64) AppleWebKit/537.36 Chrome/120.0.0.0 Safari/537.36', 'linux'],
      ['Mozilla/5.0 (X11; Linux arm64) AppleWebKit/537.36', 'linux'],
      [
        'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120.0.0.0 Safari/537.36',
        'windows',
      ],
      ['Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15', null],
      ['Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15', null],
      ['Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36', null],
      ['Mozilla/5.0 (X11; CrOS x86_64 14541.0.0) AppleWebKit/537.36', null],
      ['', null],
    ];

    for (const [ua, expected] of cases) {
      assert.equal(detectOs(ua), expected, ua);
    }
  });
});

describe('install doc downloads', () => {
  const install = fs.readFileSync(new URL('../pages/docs/install.md', import.meta.url), 'utf8');

  it('lists the same four latest-release files', () => {
    const lines = [
      `- [macOS (Apple Silicon) \`.dmg\`](${asset('Coppice-mac-arm64.dmg')})`,
      `- [macOS (Intel) \`.dmg\`](${asset('Coppice-mac-x64.dmg')})`,
      `- [Linux (x64) \`.deb\`](${asset('Coppice-linux-amd64.deb')})`,
      `- [Linux (ARM64) \`.deb\`](${asset('Coppice-linux-arm64.deb')})`,
    ];
    for (const line of lines) assert.equal(install.includes(line), true, line);
  });

  it('points readers at GitHub Releases for every build', () => {
    assert.equal(
      install.includes(
        'You can also get every build from [GitHub Releases](https://github.com/fertile-org/coppice/releases).',
      ),
      true,
    );
    assert.equal(install.includes('including the Intel Mac and Linux arm64 builds'), false);
  });
});

describe('site download buttons', () => {
  it('sends Download buttons to /download', () => {
    const root = new URL('..', import.meta.url);
    const header = fs.readFileSync(new URL('components/SiteHeader.astro', root), 'utf8');
    const links = fs.readFileSync(new URL('components/DownloadLinks.astro', root), 'utf8');
    assert.equal(header.includes("hrefFor(ctx.locale, '/download')"), true);
    assert.equal(header.includes('#download'), false);
    assert.equal(links.includes("withBase('/download')"), true);
    assert.equal(links.includes('href="#"'), false);
    assert.equal(links.includes('linux-x64.deb'), false);
  });
});
