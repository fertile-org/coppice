import { fillMissing } from './fill-missing';
import { pages as english, locale as englishLocale, type Pages } from './pages/en';

export type { Pages };

type DeepPartial<T> = T extends readonly (infer Item)[]
  ? readonly DeepPartial<Item>[]
  : T extends object
    ? { [Key in keyof T]?: DeepPartial<T[Key]> }
    : T;

const modules = import.meta.glob('./pages/*.ts', { eager: true }) as Record<
  string,
  { locale?: string; pages?: DeepPartial<Pages> }
>;

const catalogs = new Map<string, DeepPartial<Pages>>([[englishLocale, english]]);
for (const mod of Object.values(modules)) {
  if (mod.locale && mod.pages) catalogs.set(mod.locale, mod.pages);
}

/** Page copy for a locale. Missing keys stay English. */
export function pagesFor(locale: string): Pages {
  const extra = catalogs.get(locale) ?? catalogs.get(locale.toLowerCase());
  if (!extra || extra === english) return english;
  return fillMissing(english, extra);
}
