import type { Pages } from './en';

export const locale = 'fr';

/** Français homepage and download copy. Keys mirror `./en.ts`; anything missing stays English. */
export const pages: Pages = {
  home: {
    description:
      'Des agents de code, chacun avec son rôle, planifient, construisent et relisent sur un seul tableau, et rien n’est terminé tant que vous n’avez pas accepté le diff. Local, open source, avec les CLI d’agents que vous payez déjà.',
    h1: 'Recrutez des spécialistes. Gardez le dernier mot.',
    lede: 'Votre agent PM rédige le plan, et vous l’approuvez. Des agents spécialisés construisent en parallèle. Rien n’est terminé tant que vous n’avez pas accepté le diff.',
    hero: {
      stillAlt: 'Tableau Coppice avec des tickets pour le dépôt acme-web',
      videoAlt:
        'Coppice : écrivez un ticket, approuvez le plan rédigé par un agent PM, laissez les agents construire et tester, puis commentez le diff. Rien n’est terminé tant que vous ne l’avez pas accepté.',
      captions: [
        'Écrivez un ticket.',
        'Votre agent PM rédige un plan.',
        'Vous l’approuvez.',
        'Les agents construisent, relisent et testent.',
        'Relisez le diff. Commentez n’importe quelle ligne.',
        'Rien n’est terminé tant que vous ne l’avez pas accepté.',
      ],
    },
    support: 'Open source (Apache 2.0). Tourne en local, sans compte Coppice.',
    downloads: {
      mac: 'Télécharger pour macOS (Apple Silicon)',
      linux: 'Télécharger pour Linux (.deb)',
      docs: 'Lire la documentation',
    },
    howTitle: 'Comment ça marche',
    steps: [
      {
        title: 'Confiez le travail.',
        body: 'Écrivez un ticket, ou approuvez celui qu’un agent propose. Assignez-le à votre agent PM, qui rédige un plan que vous approuvez avant que quiconque n’écrive du code.',
      },
      {
        title: 'Laissez les agents construire.',
        body: 'Chaque ticket a son propre worktree git, donc les agents qui travaillent en parallèle ne touchent jamais à votre checkout principal. Suivez n’importe quel lancement en direct.',
      },
      {
        title: 'Human Review.',
        body: 'Lisez le diff dans l’application, commentez n’importe quelle ligne, et rien ne compte comme terminé tant que vous ne l’avez pas accepté.',
      },
    ],
    howClose: 'Téléchargez, ouvrez, travaillez. Rien à installer : pas de Docker, pas de compte Coppice.',
    whyTitle: 'Pourquoi Coppice',
    reasons: [
      {
        title: 'Le bon modèle pour chaque rôle.',
        body: 'Chaque agent choisit sa CLI et son modèle. Planifiez et relisez avec votre modèle le plus fort, codez avec un plus rapide, le tout sur un seul tableau.',
      },
      {
        title: 'Arrêtez de jongler entre les terminaux.',
        body: 'Le travail de chaque agent est une carte sur un seul tableau. Les tickets bloqués et le travail qui attend votre revue ressortent, donc vous savez où on a besoin de vous.',
      },
      {
        title: 'Repérez les erreurs avant qu’elles n’arrivent.',
        body: 'Chaque changement vous attend sous forme de diff dans l’application. Rien ne compte comme terminé tant que vous ne l’avez pas accepté, et ce que vous acceptez est exactement ce qui est fusionné.',
      },
      {
        title: 'Pas de deuxième abonnement.',
        body: 'Coppice pilote les CLI que vous payez déjà : Claude Code, Codex, Cursor, OpenCode et Kilo Code.',
      },
    ],
    notTitle: 'Ce que ce n’est pas',
    notItems: [
      'Pas un service de code dans le cloud',
      'Pas un remplaçant de Jira',
      'Pas « dites-le à l’agent et recevez le produit fini »',
    ],
  },
  download: {
    title: 'Télécharger Coppice',
    intro: 'Gratuit, tourne sur votre propre machine, sans compte Coppice.',
    groups: [
      {
        note: 'Vous ne savez pas quel Mac vous avez ? Ouvrez le menu Pomme → À propos de ce Mac. S’il indique une puce, choisissez Apple Silicon. S’il indique un processeur, choisissez Intel.',
        rows: [
          { name: 'macOS (Apple Silicon)', label: 'Télécharger le .dmg' },
          { name: 'macOS (Intel)', label: 'Télécharger le .dmg' },
        ],
      },
      {
        rows: [
          { name: 'Linux (x64)', label: 'Télécharger le .deb' },
          { name: 'Linux (ARM64)', label: 'Télécharger le .deb' },
        ],
      },
      {
        rows: [{ name: 'Windows 11', label: 'Installer via WSL' }],
      },
    ],
    checksum: {
      before: 'Vérifiez votre téléchargement avec ',
      sumsLabel: 'SHA256SUMS',
      between: '. Voir ',
      installLabel: 'Installer',
      after: ' pour savoir comment.',
    },
    next: {
      before: 'Ensuite, suivez les ',
      label: 'étapes d’installation',
      after: '.',
    },
  },
};
