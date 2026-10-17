/** Locale registry shared by Astro config and the site. URL prefixes are lowercase. */

export const DEFAULT_LOCALE = 'en';

export const LOCALES = [
  { path: 'en', code: 'en', name: 'English' },
  { path: 'zh-cn', code: 'zh-CN', name: '简体中文' },
  { path: 'zh-tw', code: 'zh-TW', name: '繁體中文（台灣）' },
  { path: 'zh-hk', code: 'zh-HK', name: '繁體中文（香港）' },
  { path: 'ja', code: 'ja', name: '日本語' },
  { path: 'fr', code: 'fr', name: 'Français' },
];

export const LOCALE_PATHS = LOCALES.filter((locale) => locale.path !== DEFAULT_LOCALE).map((locale) => locale.path);

/** Astro `i18n.locales`: strings when the URL prefix is the language code, otherwise path + BCP 47 code. */
export function astroLocales() {
  return LOCALES.map((locale) =>
    locale.path === locale.code ? locale.path : { path: locale.path, codes: [locale.code] },
  );
}

export function fallbackMap() {
  return Object.fromEntries(LOCALE_PATHS.map((path) => [path, DEFAULT_LOCALE]));
}
