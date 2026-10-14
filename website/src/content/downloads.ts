import { type OsId } from './download-order.ts';

export { detectOs, noJsOsOrder, orderOs, type OsId } from './download-order.ts';

/** Visitor-facing download copy. Edit this file to change the download page. */

const latest = 'https://github.com/fertile-org/coppice/releases/latest/download';

export interface DownloadAction {
  label: string;
  href: string;
  kind: 'button' | 'link';
}

export interface DownloadRow {
  name: string;
  action: DownloadAction;
}

export interface DownloadGroup {
  os: OsId;
  rows: DownloadRow[];
  note?: string;
}

export interface DownloadPageCopy {
  title: string;
  intro: string;
  groups: DownloadGroup[];
  checksum: {
    before: string;
    sumsLabel: string;
    sumsHref: string;
    between: string;
    installLabel: string;
    installHref: string;
    after: string;
  };
  next: {
    before: string;
    label: string;
    href: string;
    after: string;
  };
}

export const downloadPage: DownloadPageCopy = {
  title: 'Download Coppice',
  intro: 'Free, runs on your own machine, with no Coppice account.',
  groups: [
    {
      os: 'mac',
      note: 'Not sure which Mac you have? Open Apple menu → About This Mac. If it lists a Chip, choose Apple Silicon. If it lists a Processor, choose Intel.',
      rows: [
        {
          name: 'macOS (Apple Silicon)',
          action: {
            label: 'Download .dmg',
            href: `${latest}/Coppice-mac-arm64.dmg`,
            kind: 'button',
          },
        },
        {
          name: 'macOS (Intel)',
          action: {
            label: 'Download .dmg',
            href: `${latest}/Coppice-mac-x64.dmg`,
            kind: 'button',
          },
        },
      ],
    },
    {
      os: 'linux',
      rows: [
        {
          name: 'Linux (x64)',
          action: {
            label: 'Download .deb',
            href: `${latest}/Coppice-linux-amd64.deb`,
            kind: 'button',
          },
        },
        {
          name: 'Linux (ARM64)',
          action: {
            label: 'Download .deb',
            href: `${latest}/Coppice-linux-arm64.deb`,
            kind: 'button',
          },
        },
      ],
    },
    {
      os: 'windows',
      rows: [
        {
          name: 'Windows 11',
          action: {
            label: 'Install through WSL',
            href: '/docs/install#windows-11-through-wsl',
            kind: 'link',
          },
        },
      ],
    },
  ],
  checksum: {
    before: 'Check your download against ',
    sumsLabel: 'SHA256SUMS',
    sumsHref: `${latest}/SHA256SUMS`,
    between: '. See ',
    installLabel: 'Install',
    installHref: '/docs/install',
    after: ' for how.',
  },
  next: {
    before: 'Next, follow the ',
    label: 'install steps',
    href: '/docs/install',
    after: '.',
  },
};
