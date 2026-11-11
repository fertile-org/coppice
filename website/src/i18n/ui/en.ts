import {
  SEARCH_CLOSE,
  SEARCH_FOOTER,
  SEARCH_HINT,
  SEARCH_LABEL,
  SEARCH_PLACEHOLDER,
} from '../../lib/docs-search';
import { BLOG_INDEX } from '../../lib/blog-index';

export const locale = 'en';

/** English UI chrome. A locale file in this folder overrides these strings. */
export const ui = {
  languageLabel: 'Language',
  skip: 'Skip to content',
  nav: {
    docs: 'Docs',
    blog: 'Blog',
    github: 'GitHub',
    download: 'Download',
  },
  footer: {
    tagline: 'Hire specialists. Keep the final say.',
    licenseBefore: 'Coppice is open source under the ',
    licenseLabel: 'Apache 2.0 license',
    licenseAfter: '.',
    docs: 'Docs',
    install: 'Install',
    concepts: 'Concepts',
    providers: 'Providers',
    faq: 'FAQ',
    blog: 'Blog',
    github: 'GitHub',
    sponsor: 'Sponsor',
  },
  docs: {
    navLabel: 'Documentation',
    breadcrumbLabel: 'Breadcrumb',
    pagerLabel: 'More docs',
    previous: 'Previous',
    next: 'Next',
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
  blog: {
    description: BLOG_INDEX.description,
    empty: BLOG_INDEX.empty,
    published: 'Published',
    updated: 'Updated',
    name: 'Coppice blog',
  },
};
