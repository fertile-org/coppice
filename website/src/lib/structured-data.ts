import { AGENT_CLIS, PRIMARY_KEYWORD } from '../content/agents.ts';
import { publicHref } from '../i18n/paths.ts';

/** schema.org JSON-LD. Pages share one Organization, WebSite and SoftwareApplication, linked by `@id`. */

export type Thing = Record<string, unknown>;

const REPO = 'https://github.com/fertile-org/coppice';

export type SiteRef = { site: string; base?: string };

function url(ref: SiteRef, pathname: string): string {
  return publicHref(pathname, ref.base, ref.site);
}

export const ids = {
  organization: (ref: SiteRef) => `${url(ref, '/')}#organization`,
  website: (ref: SiteRef) => `${url(ref, '/')}#website`,
  software: (ref: SiteRef) => `${url(ref, '/')}#software`,
};

export function organization(ref: SiteRef): Thing {
  return {
    '@type': 'Organization',
    '@id': ids.organization(ref),
    name: 'Coppice',
    url: url(ref, '/'),
    logo: { '@type': 'ImageObject', url: url(ref, '/apple-touch-icon.png'), width: 180, height: 180 },
    sameAs: [REPO],
  };
}

export function website(ref: SiteRef, languages: readonly string[]): Thing {
  return {
    '@type': 'WebSite',
    '@id': ids.website(ref),
    name: 'Coppice',
    url: url(ref, '/'),
    inLanguage: [...languages],
    publisher: { '@id': ids.organization(ref) },
  };
}

export function softwareApplication(
  ref: SiteRef,
  options: { description: string; inLanguage: string; image: string; screenshot: string },
): Thing {
  return {
    '@type': 'SoftwareApplication',
    '@id': ids.software(ref),
    name: 'Coppice',
    description: options.description,
    url: url(ref, '/'),
    downloadUrl: url(ref, '/download'),
    applicationCategory: 'DeveloperApplication',
    applicationSubCategory: PRIMARY_KEYWORD,
    operatingSystem: 'macOS, Linux',
    isAccessibleForFree: true,
    offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
    license: 'https://www.apache.org/licenses/LICENSE-2.0',
    codeRepository: REPO,
    image: options.image,
    screenshot: options.screenshot,
    inLanguage: options.inLanguage,
    keywords: [PRIMARY_KEYWORD, ...AGENT_CLIS].join(', '),
    featureList: [
      'Coding agents with roles plan, build and review on one board',
      'Each ticket runs in its own git worktree',
      'Human review of every diff before it counts as done',
      `Works with ${AGENT_CLIS.join(', ')}`,
    ],
    publisher: { '@id': ids.organization(ref) },
  };
}

export type Crumb = { name: string; pathname: string };

export function breadcrumbList(ref: SiteRef, crumbs: readonly Crumb[]): Thing {
  return {
    '@type': 'BreadcrumbList',
    itemListElement: crumbs.map((crumb, index) => ({
      '@type': 'ListItem',
      position: index + 1,
      name: crumb.name,
      item: url(ref, crumb.pathname),
    })),
  };
}

export function techArticle(
  ref: SiteRef,
  options: {
    headline: string;
    description: string;
    pathname: string;
    inLanguage: string;
    image: string;
    dateModified?: string;
    keywords?: readonly string[];
  },
): Thing {
  const page = url(ref, options.pathname);
  return {
    '@type': 'TechArticle',
    '@id': `${page}#article`,
    headline: options.headline,
    description: options.description,
    url: page,
    mainEntityOfPage: page,
    inLanguage: options.inLanguage,
    image: options.image,
    ...(options.dateModified ? { dateModified: options.dateModified } : {}),
    ...(options.keywords?.length ? { keywords: options.keywords.join(', ') } : {}),
    about: { '@id': ids.software(ref) },
    isPartOf: { '@id': ids.website(ref) },
    publisher: { '@id': ids.organization(ref) },
  };
}

/** One `@graph` document. `<` is escaped so the JSON cannot close the script tag. */
export function jsonLd(items: readonly Thing[]): string {
  return JSON.stringify({ '@context': 'https://schema.org', '@graph': items }).replace(/</g, '\\u003c');
}
