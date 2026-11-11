import type { Ui } from '../ui';

export const locale = 'fr';

/** Français UI chrome. Keys mirror `./en.ts`; this replaces the whole object. */
export const ui: Ui = {
  languageLabel: 'Langue',
  skip: 'Aller au contenu',
  nav: {
    docs: 'Documentation',
    blog: 'Blog',
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
    blog: 'Blog',
    github: 'GitHub',
    sponsor: 'Sponsoriser',
  },
  docs: {
    navLabel: 'Documentation',
    breadcrumbLabel: 'Fil d’Ariane',
    pagerLabel: 'Autres pages',
    previous: 'Précédent',
    next: 'Suivant',
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
    noResults: 'Aucun résultat pour « {query} ».',
  },
  blog: {
    description:
      'Articles de l’équipe Coppice sur les agents de code, comme Claude Code et Codex, et sur la façon de les mettre au travail dans de vrais projets.',
    empty: 'Aucun article n’est publié pour le moment.',
    published: 'Publié le',
    updated: 'Mis à jour le',
    name: 'Blog Coppice',
  },
};
