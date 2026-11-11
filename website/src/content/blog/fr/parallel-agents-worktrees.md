---
title: Comment lancer plusieurs agents de code en parallèle avec des git worktrees
metaTitle: Agents en parallèle avec git worktrees
description: "Lancez plusieurs agents de code en même temps sans qu’ils s’écrasent : un git worktree par tâche, un petit script, ou un tableau qui les prépare pour vous."
answer: "Donnez à chaque agent son propre git worktree : un dossier séparé du même dépôt, sur sa propre branche, pour qu’ils ne modifient pas les mêmes fichiers. Vous pouvez les créer à la main avec git worktree add, les envelopper dans un petit script, ou utiliser un tableau comme Coppice, qui crée une branche et un worktree pour chaque ticket et montre toutes les exécutions au même endroit. Dans tous les cas, vous relisez et fusionnez chaque branche."
published: "2026-11-07"
updated: "2026-11-09"
author: Coppice
authorType: Organization
draft: false
keywords:
  - git worktree
  - agents en parallèle
  - agents de code
  - Claude Code
---

## Pourquoi c’est difficile

Deux agents dans le même dossier modifient les mêmes fichiers, et l’un casse le travail de l’autre. Les git worktrees règlent cela : chaque agent a son propre dossier et sa propre branche.

La difficulté se déplace ensuite. Vous avez un terminal par agent, vous devez retenir lequel a fini et lequel vous attend, et vous fusionnez les branches une par une. Deux tâches qui touchent les mêmes fichiers peuvent encore entrer en conflit quand vous les fusionnez.

## Option 1 : créer les worktrees à la main

Créez un worktree par tâche :

```bash
git worktree add ../my-project-login -b login
git worktree add ../my-project-search -b search
```

Ouvrez un terminal dans chaque dossier et lancez-y un agent. Quand une tâche est finie, relisez et fusionnez sa branche, puis retirez le dossier avec `git worktree remove`.

Cela convient bien pour deux ou trois tâches. Au-delà, changer de terminal et suivre chaque tâche devient le travail.

## Option 2 : l’envelopper dans un petit script

Une fonction shell peut créer le worktree et lancer l’agent en une seule étape :

```bash
agent_task() {
  dir="../$(basename "$PWD")-$1"
  git worktree add "$dir" -b "$1" && cd "$dir" && claude
}
```

Exécutez `agent_task login` depuis le dossier de votre projet. Remplacez `claude` par `codex` ou une autre CLI d’agent.

Cela évite de tout retaper au moment de la préparation. Vous avez toujours un terminal par tâche, et le suivi comme la fusion restent à votre charge.

## Option 3 : un tableau pour toutes les tâches, avec Coppice

[Coppice](/) est une application de bureau qui place vos agents de code sur un tableau de tâches. Chaque ticket reçoit sa propre branche et son propre worktree quand le travail commence, donc votre checkout principal n’est pas touché. Plusieurs tickets peuvent tourner en même temps, et le tableau montre à quelle étape se trouve chacun.

1. [Installez Coppice](/docs/install) et ajoutez votre projet.
2. Créez vos agents, et choisissez Claude Code, Codex ou une autre CLI pour chacun.
3. Écrivez un ticket par tâche, puis lancez l’agent sur chacun.
4. Approuvez chaque plan, puis relisez chaque diff quand il est prêt.
5. Fusionnez les tickets un par un. Si une fusion entre en conflit, Coppice liste les fichiers, et vous pouvez demander à un agent de les résoudre.

![Le panneau Git actions d’un ticket, qui montre sa propre branche, agent/TICKET-723e8f0a, et son dossier worktree.](/blog/parallel-agents-worktrees/branch-and-worktree.png "Chaque ticket a sa propre branche et son propre worktree.")

[Concepts](/docs/concepts) explique chaque étape que traverse un ticket.

## Séparez les tâches pour qu’elles fusionnent proprement

Quelle que soit l’option :

- Donnez à chaque tâche ses propres fichiers. Les fichiers partagés, comme les lockfiles, les schémas ou un module partagé, appartiennent à une seule tâche, ou à une tâche plus tardive.
- Fusionnez une branche, mettez la suivante à jour sur ce résultat, et vérifiez que les deux changements ont encore du sens ensemble. Git peut fusionner deux fichiers proprement même lorsque leur contenu se contredit.

## Lequel choisir

- **Deux ou trois tâches :** créez les worktrees à la main.
- **Vous lancez souvent des tâches et vous aimez le terminal :** un petit script.
- **Plusieurs tâches à la fois, et vous voulez un seul endroit pour les voir, les relire et les fusionner :** Coppice. Commencez par [Premiers pas](/docs/getting-started).
