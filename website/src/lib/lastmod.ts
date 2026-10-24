import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';

function git(args: string[]): string | null {
  try {
    return execFileSync('git', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
  } catch {
    return null;
  }
}

let boundaries: Set<string> | undefined;

/** Commits at the edge of a shallow clone look like they touched every file, so their dates mean nothing. */
function shallowBoundaries(): Set<string> {
  if (boundaries) return boundaries;
  const file = git(['rev-parse', '--git-path', 'shallow']);
  boundaries = new Set(
    file && existsSync(file)
      ? readFileSync(file, 'utf8')
          .split('\n')
          .map((line) => line.trim())
          .filter(Boolean)
      : [],
  );
  return boundaries;
}

/**
 * Commit date (`YYYY-MM-DD`) of the newest commit touching any of `files`
 * (paths relative to the website root). Undefined when git has no reliable answer.
 */
export function lastCommitDate(files: readonly string[]): string | undefined {
  const existing = files.filter((file) => existsSync(path.resolve(file)));
  if (!existing.length) return undefined;
  const line = git(['log', '-1', '--format=%H %cI', '--', ...existing]);
  if (!line) return undefined;
  const [hash, date] = line.split(' ');
  if (!hash || !date || shallowBoundaries().has(hash)) return undefined;
  return date.slice(0, 10);
}

/** Source files whose edits change the page at `englishPath` in `locale`. */
export function pageSources(locale: string, englishPath: string): string[] {
  const english = locale === 'en';
  if (englishPath === '/' || englishPath === '/download') {
    const component = englishPath === '/' ? 'src/components/HomePage.astro' : 'src/components/DownloadPage.astro';
    return [component, `src/i18n/pages/${english ? 'en' : locale}.ts`];
  }
  const slug = englishPath === '/docs' ? 'index' : englishPath.slice('/docs/'.length);
  if (english) return [`src/pages/docs/${slug}.md`, `src/pages/docs/${slug}.mdx`];
  return ['md', 'mdx'].flatMap((ext) => [
    `src/content/docs/${locale}/${slug}.${ext}`,
    `src/pages/${locale}/docs/${slug}.${ext}`,
  ]);
}
