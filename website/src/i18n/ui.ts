import { noResultsMessage } from '../lib/docs-search';
import { ui as english, locale as englishLocale } from './ui/en';

export type Ui = typeof english;

const modules = import.meta.glob('./ui/*.ts', { eager: true }) as Record<string, { locale?: string; ui?: Ui }>;

const catalogs = new Map<string, Ui>([[englishLocale, english]]);
for (const mod of Object.values(modules)) {
  if (mod.locale && mod.ui) catalogs.set(mod.locale, mod.ui);
}

export function uiFor(locale: string): Ui {
  return catalogs.get(locale) ?? catalogs.get(locale.toLowerCase()) ?? english;
}

export function noResultsFor(lang: string, query: string): string {
  const ui = uiFor(lang.toLowerCase());
  if (ui === english || ui.search.noResults === english.search.noResults) return noResultsMessage(query);
  return ui.search.noResults.replaceAll('{query}', query);
}
