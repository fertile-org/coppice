import type { Ui } from '../ui';

export const locale = 'fr';

/** Français UI chrome. Keys mirror `./en.ts`; this replaces the whole object. */
export const ui: Ui = {
  languageLabel: 'Langue',
  skip: 'Aller au contenu',
  nav: {
    docs: 'Documentation',
    github: 'GitHub',
    download: 'Télécharger',
  },
  footer: {
    tagline: 'Recrutez des spécialistes. Gardez le dernier mot.',
    licenseBefore: 'Coppice est open source sous ',
    licenseLabel: 'licence Apache 2.0',
    licenseAfter: '.',
    docs: 'Documentation',
    install: 'Installer',
    concepts: 'Concepts',
    providers: 'Fournisseurs',
    faq: 'FAQ',
    github: 'GitHub',
  },
  docs: {
    navLabel: 'Documentation',
    install: 'Installer',
    gettingStarted: 'Premiers pas',
    concepts: 'Concepts',
    providers: 'Fournisseurs',
    plugins: 'Plugins',
    faq: 'FAQ',
    advanced: 'Avancé',
    settings: 'Paramètres',
    shortcuts: 'Raccourcis clavier',
    backup: 'Sauvegarder les données',
  },
  search: {
    placeholder: 'Rechercher dans la doc',
    label: 'Rechercher dans la doc',
    hint: '/',
    footer: '↑↓ pour naviguer, Entrée pour ouvrir, Échap pour fermer',
    close: 'Fermer',
    noResults: 'Aucun résultat pour « {query} ».',
  },
};
