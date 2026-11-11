---
title: Comment relire le code généré par une IA avant de fusionner
metaTitle: Relire le code d’un agent avant de fusionner
description: "Quoi vérifier dans le diff d’un agent avant de fusionner, et trois façons de relire : dans git, avec un second agent, ou avec un plan approuvé d’abord."
answer: "Comparez le diff à ce que vous avez demandé : fichiers qu’il ne devrait pas toucher, gestion des erreurs, nouvelles dépendances, et tests qui couvrent vraiment le changement. Vous pouvez relire dans git ou une pull request, demander une première passe à un second agent, ou utiliser un tableau comme Coppice, où vous approuvez un plan avant tout code et relisez le diff par rapport à ce plan avant la fusion."
published: "2026-11-07"
updated: "2026-11-09"
author: Coppice
authorType: Organization
draft: false
keywords:
  - revue de code IA
  - agents de code
  - revue de code
  - fusion
---

## Pourquoi c’est difficile

Les agents écrivent du code plus vite que vous ne pouvez le lire. Une petite demande peut revenir en un grand diff, avec des changements que vous n’avez pas demandés, et un résumé qui dit que tout fonctionne. Lire chaque ligne prend trop de temps. Survoler laisse passer des erreurs.

La partie difficile est de savoir ce que l’agent devait faire. Sans cela, vous ne pouvez pas dire si un changement a sa place dans le diff, ou s’il s’agit d’un périmètre que personne n’a demandé.

## Ce qu’il faut vérifier dans le diff

Quelle que soit la façon de relire, cherchez ceci :

- **Le périmètre.** Des fichiers que vous n’attendiez pas, comme des lockfiles, des migrations, de la configuration ou des renommages.
- **Les chemins d’erreur.** Ce qui se passe quand une entrée manque, qu’un appel échoue, ou qu’une valeur est vide.
- **Les dépendances et l’accès.** De nouveaux paquets, scripts, identifiants, ou des changements sur qui peut lire ou modifier les données.
- **Les tests.** Des tests qui exercent le changement, pas seulement une ligne du résumé qui dit qu’ils passent.

## Option 1 : relire dans git ou une pull request

Faites le diff de la branche de l’agent avec votre branche principale, ou ouvrez une pull request et relisez-la comme le travail d’un collègue. Cela fonctionne avec n’importe quel agent et n’importe quel outil que vous utilisez déjà.

La limite est que vous ne voyez que le résultat. Ce que vous avez demandé reste dans l’historique du chat, donc vous comparez le diff à votre souvenir de la demande.

## Option 2 : demander d’abord à un second agent

Avant de relire vous-même, demandez à un autre agent de relire le diff, par exemple Codex qui relit ce que Claude Code a écrit. Il repère les problèmes évidents et vous donne une liste d’endroits à regarder.

Sa relecture est un second avis, pas une validation. Vous lisez encore le diff, et vous décidez.

## Option 3 : approuver le plan d’abord, avec Coppice

[Coppice](/) est une application de bureau qui place vos agents de code sur un tableau de tâches. Avant qu’aucun code ne soit écrit, un agent écrit un plan et vous l’approuvez. Quand le travail est fini, Coppice montre le diff à côté du plan approuvé et signale tout fichier que le plan ne mentionnait pas. Accepter le travail ne le fusionne pas : vous choisissez quand il atteint votre branche principale.

1. [Installez Coppice](/docs/install) et ajoutez votre projet.
2. Écrivez un ticket et lancez l’agent. Un agent PM écrit le plan.
3. Lisez le plan. Approuvez-le, ou demandez des changements.
4. Quand le ticket arrive à Wait for Human Review, relisez le code. Commentez des lignes, ou acceptez le travail.
5. Fusionnez quand vous êtes prêt.

Voici à quoi cela ressemble. Le ticket demande un interrupteur de mode sombre dans Settings. Le plan de l’agent PM a trois étapes : ajouter l’interrupteur, utiliser le theme hook existant, et ajouter un test des réglages.

![Un ticket intitulé Add a dark mode toggle to Settings, en Plan Review. Le plan proposé construit un interrupteur dans la ligne Settings, avec trois étapes : ajouter l’interrupteur Settings, utiliser le theme hook existant, et ajouter un test des réglages.](/blog/review-before-merging/plan-review.png "Le plan, avant qu’aucun code ne soit écrit.")

Quand le travail revient, le plan approuvé est à côté du diff. Deux choses ressortent. Le nouveau composant garde son propre état de thème au lieu d’utiliser le theme hook existant, et un seul fichier a changé, donc il n’y a pas de test des réglages. Les deux méritent un commentaire avant que vous acceptiez.

![Revue de code du même ticket. Le plan approuvé est à gauche, src/settings/ThemeToggle.tsx est le seul fichier modifié, et un commentaire sous la ligne useState demande d’utiliser le theme hook existant.](/blog/review-before-merging/diff-before-accept.png "Le diff à côté du plan approuvé, avec un commentaire sur la ligne qui ignore le theme hook.")

[Concepts](/docs/concepts) explique chaque étape que traverse un ticket.

## Lequel choisir

- **Un petit changement d’un seul agent :** relisez-le dans git ou une pull request.
- **Un changement plus grand, et vous voulez une première passe :** demandez à un second agent, puis lisez vous-même.
- **Beaucoup de tâches d’agents, et vous voulez vous accorder sur le plan avant le code :** Coppice. Commencez par [Premiers pas](/docs/getting-started).
