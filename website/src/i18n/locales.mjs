/** Locale registry shared by Astro config and the site. URL prefixes are lowercase. `og` is the Open Graph locale. */

export const DEFAULT_LOCALE = 'en';

export const LOCALES = [
  { path: 'en', code: 'en', name: 'English', og: 'en_US' },
  { path: 'vi', code: 'vi', name: 'Tiếng Việt', og: 'vi_VN' },
  { path: 'zh-cn', code: 'zh-CN', name: '简体中文', og: 'zh_CN' },
  { path: 'zh-tw', code: 'zh-TW', name: '繁體中文（台灣）', og: 'zh_TW' },
  { path: 'zh-hk', code: 'zh-HK', name: '繁體中文（香港）', og: 'zh_HK' },
  { path: 'ja', code: 'ja', name: '日本語', og: 'ja_JP' },
  { path: 'fr', code: 'fr', name: 'Français', og: 'fr_FR' },
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
