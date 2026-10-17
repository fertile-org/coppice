import {
  SEARCH_CLOSE,
  SEARCH_FOOTER,
  SEARCH_HINT,
  SEARCH_LABEL,
  SEARCH_PLACEHOLDER,
} from '../../lib/docs-search';

export const locale = 'en';

/** English UI chrome. A locale file in this folder overrides these strings. */
export const ui = {
  languageLabel: 'Language',
  skip: 'Skip to content',
  nav: {
    docs: 'Docs',
    github: 'GitHub',
    download: 'Download',
  },
  footer: {
    tagline: 'Put your agents on a board. Keep the gates.',
    licenseBefore: 'Coppice is open source under the ',
    licenseLabel: 'Apache 2.0 license',
    licenseAfter: '.',
    docs: 'Docs',
    install: 'Install',
    concepts: 'Concepts',
    providers: 'Providers',
    faq: 'FAQ',
    github: 'GitHub',
  },
  docs: {
    navLabel: 'Documentation',
    install: 'Install',
    gettingStarted: 'Getting started',
    concepts: 'Concepts',
    providers: 'Providers',
    plugins: 'Plugins',
    faq: 'FAQ',
    advanced: 'Advanced',
    settings: 'Settings',
    shortcuts: 'Keyboard shortcuts',
    backup: 'Backup data',
  },
  search: {
    placeholder: SEARCH_PLACEHOLDER,
    label: SEARCH_LABEL,
    hint: SEARCH_HINT,
    footer: SEARCH_FOOTER,
    close: SEARCH_CLOSE,
    noResults: 'No results for "{query}".',
  },
};
