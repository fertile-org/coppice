import { pages as englishPages, type Pages } from '../i18n/pages/en.ts';
import { type OsId } from './download-order.ts';

export { detectOs, noJsOsOrder, orderOs, type OsId } from './download-order.ts';

/** Download URLs. The words live in `src/i18n/pages`. */

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

const linkSkeleton = [
  {
    os: 'mac' as const,
    rows: [
      { href: `${latest}/Coppice-mac-arm64.dmg`, kind: 'button' as const },
      { href: `${latest}/Coppice-mac-x64.dmg`, kind: 'button' as const },
    ],
  },
  {
    os: 'linux' as const,
    rows: [
      { href: `${latest}/Coppice-linux-amd64.deb`, kind: 'button' as const },
      { href: `${latest}/Coppice-linux-arm64.deb`, kind: 'button' as const },
    ],
  },
  {
    os: 'windows' as const,
    rows: [{ href: '/docs/install#windows-11-through-wsl', kind: 'link' as const }],
  },
];

/** Words from a locale, links from the shared skeleton. */
export function downloadPageFor(wording: Pages['download']): DownloadPageCopy {
  return {
    title: wording.title,
    intro: wording.intro,
    groups: linkSkeleton.map((group, index) => {
      const text = wording.groups[index];
      return {
        os: group.os,
        note: text?.note,
        rows: group.rows.map((row, rowIndex) => ({
          name: text?.rows[rowIndex]?.name ?? '',
          action: {
            label: text?.rows[rowIndex]?.label ?? '',
            href: row.href,
            kind: row.kind,
          },
        })),
      };
    }),
    checksum: {
      before: wording.checksum.before,
      sumsLabel: wording.checksum.sumsLabel,
      sumsHref: `${latest}/SHA256SUMS`,
      between: wording.checksum.between,
      installLabel: wording.checksum.installLabel,
      installHref: '/docs/install',
      after: wording.checksum.after,
    },
    next: {
      before: wording.next.before,
      label: wording.next.label,
      href: '/docs/install',
      after: wording.next.after,
    },
  };
}

export const downloadPage: DownloadPageCopy = downloadPageFor(englishPages.download);
