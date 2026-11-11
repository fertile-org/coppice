---
title: Comment utiliser Claude Code et Codex sur le même projet
metaTitle: Claude Code et Codex sur un projet
description: "Trois façons de lancer Claude Code et Codex sur le même code : deux terminaux, des git worktrees, ou un tableau où chaque agent prend ses tickets."
answer: "Donnez à chaque outil sa propre copie du code, pour qu’ils ne modifient pas les mêmes fichiers en même temps. Vous pouvez le faire à la main avec des git worktrees, ou utiliser un tableau comme Coppice, où chaque ticket a sa branche et où vous choisissez Claude Code ou Codex. Dans les deux cas, vous relisez les changements avant qu’ils n’atteignent la branche principale."
published: "2026-11-07"
updated: "2026-11-08"
draft: false
author: Coppice
authorType: Organization
keywords:
  - Claude Code
  - Codex
  - agents de code
  - git worktree
---

## Pourquoi les utiliser ensemble

Claude Code et Codex sont deux agents de code qui tournent dans votre terminal. Vous pouvez confier le plan à l’un et le code à l’autre, ou demander à un second modèle de relire ce que le premier a changé. Le plus dur n’est pas de les lancer. C’est de garder leurs changements séparés, pour savoir qui a modifié quoi.

Il y a trois façons courantes de s’y prendre.

## Option 1 : deux terminaux, un seul checkout

Ouvrez votre projet dans deux terminaux. Exécutez `claude` dans l’un et `codex` dans l’autre.

C’est le moyen le plus rapide de commencer. Cela convient quand vous n’utilisez qu’un outil à la fois, par exemple Codex pour relire ce que Claude Code vient d’écrire. Si les deux modifient en même temps, ils peuvent changer le même fichier, et l’un peut écraser le travail de l’autre.

## Option 2 : un git worktree par outil

Un git worktree est un second dossier de travail pour le même dépôt, sur sa propre branche. Créez-en un pour Codex :

```bash
git worktree add ../my-project-codex -b codex-task
```

Exécutez `claude` dans votre dossier principal et `codex` dans `../my-project-codex`. Les deux peuvent travailler en même temps sans toucher aux fichiers de l’autre. Quand ils ont fini, vous relisez chaque branche et vous la fusionnez vous-même.

Le travail reste séparé, mais la coordination vous revient : vous décidez quel outil reçoit quelle tâche, vous passez le contexte de l’un à l’autre, et vous nettoyez les branches.

## Option 3 : un tableau pour les deux, avec Coppice

[Coppice](/) est une application de bureau qui place vos agents de code sur un tableau de tâches. Il exécute les commandes `claude` et `codex` déjà présentes sur votre ordinateur. Chaque ticket tourne sur sa propre branche, donc votre checkout principal n’est pas touché, et les agents se passent les tickets par le tableau.

1. [Installez Coppice](/docs/install). Connectez-vous à Claude Code et à Codex dans le terminal, comme d’habitude.
2. Ajoutez le dossier de votre projet.
3. Créez vos agents, et choisissez Claude Code ou Codex pour chacun. Par exemple, un PM et un Technical Lead sur Claude Code, un Backend Engineer sur Codex, et un Reviewer sur Claude Code, pour que Claude Code relise ce que Codex écrit.
4. Écrivez un ticket et lancez l’agent.

L’agent PM écrit le plan, et vous l’approuvez. Les agents construisent ensuite le changement et le relisent. Vous lisez le diff final, vous l’acceptez, et vous fusionnez quand vous êtes prêt.

![Formulaire de nouvel agent. Le préréglage PM est sélectionné, et la CLI de l’agent est Claude Code.](/blog/claude-code-and-codex/new-agent.png "Chaque agent tourne sur une seule CLI. Choisissez Claude Code ou Codex à la création.")

## Lequel choisir

- **Une tâche rapide, un seul outil à la fois :** deux terminaux.
- **Du travail en parallèle, et vous gérez vous-même les branches :** des git worktrees.
- **Plusieurs tickets, des agents qui se passent le travail et se relisent, et vous approuvez le plan et le diff :** Coppice. Commencez par [Premiers pas](/docs/getting-started).
