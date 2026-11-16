import type { Pages } from './en';

export const locale = 'fr';

/** Français homepage and download copy. Keys mirror `./en.ts`; anything missing stays English. */
export const pages: Pages = {
  home: {
    title: 'Coppice : application de bureau pour le codage multi-agent',
    description:
      'Coppice est une application de bureau pour le codage multi-agent. Vous approuvez le plan et le diff. Claude Code, Codex et d’autres tournent sur votre machine.',
    worksWith: 'Fonctionne avec',
    h1: 'Recrutez des spécialistes. Gardez le dernier mot.',
    lede: 'Une application de bureau pour le codage multi-agent. Votre agent PM rédige le plan, et vous l’approuvez. Des agents spécialisés construisent en parallèle. Rien n’est terminé tant que vous n’avez pas accepté le diff.',
    hero: {
      stillAlt: 'Tableau Coppice avec des tickets pour le dépôt acme-web',
      videoAlt:
        'Coppice : écrivez un ticket, comparez les options de design, approuvez le plan rédigé par un agent PM, laissez les agents construire et tester, puis commentez le diff. Rien n’est terminé tant que vous ne l’avez pas accepté.',
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
        title: 'Planifier avec l’agent PM',
        body: 'Votre agent PM lit le ticket et rédige un plan : l’approche, les étapes, et comment le changement sera vérifié. Vous approuvez ce plan avant que le code commence.',
        imageAlt:
          'Plan Review pour un interrupteur de mode sombre. L’agent PM est assigné, et le plan proposé liste l’approche et les étapes.',
      },
      {
        title: 'Approuver le design',
        body: 'Quand le ticket change ce que les gens voient, l’agent montre quelques mises en page et indique celle qu’il recommande. Vous en choisissez une, et le plan suit ce choix.',
        imageAlt:
          'Options de design pour le contrôle de thème des Settings, avec trois mises en page et un aperçu de l’option recommandée.',
      },
      {
        title: 'Laisser les agents construire',
        body: 'Des agents spécialisés construisent, relisent et testent le ticket. Chaque ticket a son propre worktree git, donc le travail en parallèle reste hors de votre checkout principal. Vous pouvez suivre un lancement pendant qu’il avance.',
        imageAlt: 'Le tableau, avec des tickets en cours, en revue, en QA, et un ticket qui attend une personne.',
      },
      {
        title: 'Relire les changements de code',
        body: 'Lisez le diff dans l’application et commentez n’importe quelle ligne. Rien ne compte comme terminé tant que vous ne l’avez pas accepté. Accepter et Merge restent deux choix distincts.',
        imageAlt:
          'Revue de code d’un interrupteur de thème : le plan approuvé, le fichier modifié, et un commentaire sur une ligne du diff.',
      },
    ],
    howClose: 'Téléchargez, ouvrez, travaillez. Rien à installer : pas de Docker, pas de compte Coppice.',
    whyTitle: 'Pourquoi Coppice',
    reasons: [
      {
        title: 'Le bon modèle pour chaque rôle.',
        body: 'Chaque agent choisit sa CLI et son modèle. Planifiez et relisez avec votre modèle le plus fort, et codez avec un modèle plus rapide, le tout sur un seul tableau.',
      },
      {
        title: 'Arrêtez de jongler entre les terminaux.',
        body: 'Le travail de chaque agent est une carte sur un seul tableau. Les tickets bloqués et le travail qui attend votre revue ressortent. Vous savez donc où intervenir.',
      },
      {
        title: 'Repérez les erreurs avant qu’elles n’arrivent.',
        body: 'Chaque changement vous attend sous forme de diff dans l’application. Rien ne compte comme terminé tant que vous ne l’avez pas accepté. Ce que vous acceptez est exactement ce qui est fusionné.',
      },
      {
        title: 'Pas de deuxième abonnement.',
        body: 'Coppice pilote les CLI que vous payez déjà : Claude Code, Codex, Cursor, OpenCode, Kilo Code, GitHub Copilot, Qwen Code, Kimi Code et Grok.',
      },
    ],
    notTitle: 'Ce que ce n’est pas',
    notItems: [
      'Pas un service de code dans le cloud',
      'Pas un remplaçant de Jira',
      'Pas « dites-le à l’agent et recevez le produit fini »',
    ],
  },
  download: {
    title: 'Télécharger Coppice',
    metaTitle: 'Télécharger Coppice pour macOS et Linux',
    description:
      'Téléchargez Coppice, l’application de bureau gratuite et open source pour le codage multi-agent, pour macOS et Linux. Tourne en local, sans compte.',
    intro: 'Gratuit, tourne sur votre propre machine, sans compte Coppice.',
    groups: [
      {
        note: 'Pour savoir quel Mac vous avez, ouvrez le menu Pomme → À propos de ce Mac. S’il indique une puce, choisissez Apple Silicon. S’il indique un processeur, choisissez Intel.',
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
