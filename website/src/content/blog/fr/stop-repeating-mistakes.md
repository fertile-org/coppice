---
title: Comment empêcher un agent de code de répéter les mêmes erreurs
metaTitle: Empêcher un agent de répéter ses erreurs
description: "Placez la correction dans un fichier de règles, une vérification, la tâche ou un skill. Approuvez une note Knowledge pour qu’un ticket correspondant la liste."
answer: "Écrivez la correction là où la session suivante la verra. Un fichier de règles comme CLAUDE.md, AGENTS.md ou une règle Cursor est chargé dans les sessions suivantes. Un test ou une vérification lint échoue quand l’erreur revient. Vous pouvez aussi la mettre dans la tâche, ou dans un skill que l’agent charge quand la tâche convient. Dans Coppice, enregistrez-la comme note Knowledge et approuvez-la. Sur un ticket ultérieur dont les mots correspondent, on dit à l’agent de lire cette note."
published: "2026-11-11"
author: Coppice
authorType: Organization
draft: false
keywords:
  - agents de code IA
  - CLAUDE.md
  - AGENTS.md
  - règles Cursor
  - Knowledge
---

## Pourquoi c’est difficile

Vous corrigez l’agent. La tâche suivante repart de zéro, et l’erreur revient. Un agent de code ne garde pas cette correction une fois le chat terminé. Il faut la placer devant la prochaine exécution, ou faire échouer une vérification quand l’erreur revient.

## Option 1 : un fichier de règles dans le projet

Claude Code lit `CLAUDE.md` au début de chaque session, depuis le dossier où vous démarrez et les dossiers au-dessus. Quand ce chemin n’a ni `CLAUDE.md` ni `CLAUDE.local.md`, il lit `AGENTS.md` à la place. Il charge aussi les 200 premières lignes de son fichier de mémoire automatique, `MEMORY.md`, ou 25KB, selon ce qui arrive en premier. Claude Code traite ces fichiers comme du contexte. Ils ne bloquent pas une modification.

Codex lit `AGENTS.md` avant de travailler. Il part de la racine du projet jusqu’au dossier où vous avez démarré, et s’arrête quand les fichiers combinés atteignent 32 KiB.

Cursor lit `AGENTS.md` à la racine du projet et dans les sous-dossiers des fichiers sur lesquels vous travaillez. Une règle de projet dans `.cursor/rules` est dans chaque chat quand vous la réglez sur always apply. Sinon, Cursor l’inclut quand les fichiers correspondent, quand l’agent juge qu’elle convient, ou quand vous la mentionnez. Un fichier de règles est un conseil. L’agent peut sauter une ligne.

## Option 2 : un test ou une vérification lint

Quand une vérification peut voir l’erreur, ajoutez-la. Un test qui échoue quand un contrôle Settings garde son propre état de thème, ou une règle lint pour ce motif, arrête le changement à la prochaine exécution.

Cela bloque l’erreur. Un choix de mise en page demande encore une personne, parce qu’une vérification ne peut pas le voir.

## Option 3 : la mettre dans la tâche, ou dans un skill

Collez la correction dans cette tâche. L’agent la voit pour cette exécution. La tâche suivante a besoin du même collage.

Un skill est un court fichier d’instructions que l’agent charge quand la tâche convient, ou quand vous l’invoquez. Dans Claude Code, le fichier reste hors de la session jusque-là : c’est pourquoi une règle d’une ligne va dans `CLAUDE.md` et une procédure dans un skill. L’agent peut encore le sauter.

## Option 4 : une note Knowledge que vous approuvez, avec Coppice

[Coppice](/) est une application de bureau qui place vos agents de code sur un tableau de tâches. Knowledge est une note sur le projet pour les tickets suivants. Vous écrivez la note, ou, pendant un ticket, l’agent peut en proposer une pour une règle dont un ticket ultérieur aurait besoin. Elle reste en attente tant que vous ne l’avez pas approuvée. Les exécutions suivantes ne voient pas une note en attente.

Une note peut couvrir tous les tableaux, un seul tableau, ou un agent sur un tableau. Une note que l’agent propose depuis un ticket couvre ce tableau.

1. [Installez Coppice](/docs/install) et ajoutez votre projet.
2. Ouvrez Knowledge et ajoutez une note. Pour un interrupteur de mode sombre dans Settings, écrivez que le contrôle utilise le theme hook existant, et que le changement inclut un test des réglages. Choisissez le tableau où se trouve le travail.
3. Approuvez la note.
4. Sur le ticket suivant, lancez l’agent. Coppice cherche dans les notes approuvées les mots du titre et de la description du ticket. Une correspondance est listée pour l’agent au début de l’exécution, et on lui dit de la lire avant de s’y fier.

Un ticket ultérieur intitulé Add a dark mode toggle to Settings partage ces mots, donc la note est listée. Le texte complet reste hors du prompt tant que l’agent ne l’a pas lu. L’agent peut aussi chercher dans les notes approuvées. Un terminal en dehors de Coppice ne voit jamais la note. [Concepts](/docs/concepts#knowledge) décrit Knowledge.

![Dialogue d’ajout Knowledge. Le titre est Settings theme controls use the existing theme hook, le type est Review feedback, le tableau est acme-web, et la note dit qu’un interrupteur de mode sombre utilise le theme hook existant et inclut un test des réglages.](/blog/stop-repeating-mistakes/add-note.png "La note nomme le theme hook, sur le tableau acme-web.")

![Une note Knowledge en attente sur le tableau acme-web. Le titre est Settings theme controls use the existing theme hook, et Approve est sur la carte.](/blog/stop-repeating-mistakes/pending-note.png "La note reste en attente tant que vous ne l’approuvez pas.")

## Lequel choisir

- **L’erreur peut faire échouer une vérification :** ajoutez un test ou une règle lint.
- **Vous voulez qu’une règle courte soit vue à chaque session d’un outil :** `CLAUDE.md`, `AGENTS.md`, ou une règle Cursor.
- **Une seule tâche, ou une procédure que vous ne voulez pas dans chaque session :** écrivez-la dans la tâche, ou mettez-la dans un skill.
- **Beaucoup de tickets, et vous voulez approuver ce qui est gardé avant que la prochaine exécution puisse l’utiliser :** Coppice. Commencez par [Premiers pas](/docs/getting-started).
