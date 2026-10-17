import { withBase } from '../lib/url';
import {
  alternateLinks,
  canonicalHref,
  localeCode,
  localeFromPathname,
  localizedPath,
  translatedLocales,
  type LocalePath,
} from './paths';
import { catalog } from './translations';
import { uiFor, type Ui } from './ui';

export type PageI18n = {
  locale: LocalePath;
  code: string;
  englishPath: string;
  rewritten: boolean;
  translated: boolean;
  alternates: { hreflang: string; href: string }[];
  canonical: string;
  ui: Ui;
};

export function hrefFor(locale: string, englishPath: string): string {
  return withBase(localizedPath(locale, englishPath));
}

export function useI18n(astro: { originPathname: string; url: URL; site?: URL | undefined }): PageI18n {
  const base = import.meta.env.BASE_URL || '/';
  const site = astro.site?.href;
  const origin = localeFromPathname(astro.originPathname, base);
  const rendered = localeFromPathname(astro.url.pathname, base);
  const translated =
    origin.locale === 'en' || (catalog.get(origin.englishPath)?.has(origin.locale) ?? false);
  return {
    locale: origin.locale,
    code: localeCode(origin.locale),
    englishPath: origin.englishPath,
    rewritten: origin.locale !== rendered.locale || origin.englishPath !== rendered.englishPath,
    translated,
    alternates: alternateLinks(origin.englishPath, translatedLocales(catalog, origin.englishPath), base, site),
    canonical: canonicalHref(origin.locale, origin.englishPath, translated, base, site),
    ui: uiFor(origin.locale),
  };
}
