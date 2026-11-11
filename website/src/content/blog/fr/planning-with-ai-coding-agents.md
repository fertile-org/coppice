---
title: Comment planifier avec des agents de code avant d’écrire
metaTitle: Planifier avec des agents de code
description: "Un plan court avant que l’agent modifie le code : ce qu’un bon plan contient, et trois façons de faire, du mode plan à une étape que vous approuvez."
answer: "Demandez à l’agent un plan court avant qu’il ne modifie le code : l’approche, les étapes, et comment le changement sera testé. Laissez-le poser des questions d’abord, et comparer les options quand il y a un vrai choix. Vous pouvez le faire avec le mode plan d’une CLI, avec un fichier de plan dans le dépôt, ou sur un tableau comme Coppice, où le plan est une étape séparée que vous approuvez et où le code est ensuite relu par rapport à ce plan."
published: "2026-11-09"
draft: false
author: Coppice
authorType: Organization
keywords:
  - agents de code
  - planification
  - Claude Code
  - Codex
---

## Pourquoi planifier d’abord

Quand un agent passe directement au code, un malentendu n’apparaît que dans le diff. L’agent a alors déjà construit la mauvaise chose, et vous passez votre temps à la lire, à expliquer ce que vous vouliez, et à attendre une autre tentative. Un plan montre le même malentendu en quelques lignes, lisibles en une minute.

Obtenir un plan est facile : tout agent en écrit un si vous le demandez. Le rendre utile est la partie difficile. La demande est vague, l’agent devine au lieu de poser des questions, le plan liste des étapes sans les décisions qui les justifient, et une fois le code commencé, personne ne le vérifie par rapport au plan.

## Ce qu’un bon plan contient

- **Le résultat et les contraintes, dans vos mots.** « Settings reçoit un interrupteur de mode sombre qui suit le même réglage de thème que le reste de l’application. »
- **Des réponses, pas des suppositions.** L’agent demande ce qu’il doit savoir avant de planifier.
- **L’approche, et le choix qui la motive.** Quand il y a un vrai choix, comme une disposition ou un modèle de données, l’agent montre les options et en recommande une.
- **Quelques étapes, y compris le test** qui montre que le changement fonctionne.
- **Ce qu’il ne changera pas.**

Si vous ne pouvez pas lire le plan en une ou deux minutes, la tâche est probablement trop grande. Divisez-la.

## Option 1 : le mode plan dans la CLI

Claude Code a un mode plan : l’agent lit le code et propose un plan sans modifier de fichiers. Avec les autres agents, demandez un plan et dites à l’agent de ne rien modifier tant que vous ne l’avez pas approuvé.

C’est le moyen le plus rapide de commencer. La limite est que le plan vit dans la conversation. Quand la session se termine, le plan disparaît, et vous ne pouvez pas facilement vérifier le diff final par rapport à lui.

## Option 2 : un fichier de plan dans le dépôt

Demandez à l’agent d’écrire le plan dans un fichier Markdown, par exemple `docs/plans/dark-mode.md`. Modifiez le fichier vous-même, puis lancez l’implémentation avec « Implement docs/plans/dark-mode.md », dans le même agent ou dans un autre.

Le plan survit d’une session à l’autre et d’un agent à l’autre, et vous pouvez le relire comme du code. Vous gérez encore les fichiers, vous décidez quand un plan est approuvé, et vous comparez le diff au plan à la main.

## Option 3 : une étape de plan que vous approuvez, avec Coppice

[Coppice](/) est une application de bureau qui place vos agents de code sur un tableau de tâches. Chaque ticket commence par un plan. Un agent PM peut d’abord vous poser jusqu’à trois questions, puis il écrit le plan. Pour un changement d’interface, il montre aussi des options de conception que vous pouvez comparer et choisir. L’exécution de planification ne modifie pas votre code. Une fois le plan approuvé, il est enregistré sur le ticket, les agents construisent à partir de lui, et vous relisez le diff à côté.

1. [Installez Coppice](/docs/install) et ajoutez votre projet.
2. Écrivez un ticket avec le résultat et les contraintes, puis lancez l’agent.
3. Répondez aux questions de l’agent PM, s’il en pose.
4. Lisez le plan. Pour un changement d’interface, ouvrez les options de conception et choisissez-en une. Ensuite, approuvez le plan, ou demandez des changements.
5. Quand le code est prêt, relisez le diff par rapport au plan approuvé.

Pour un ticket qui demande un interrupteur de mode sombre dans Settings, le plan nomme l’approche et trois étapes, dont un test des réglages :

![Un ticket intitulé Add a dark mode toggle to Settings, en Plan Review. Le plan proposé construit un interrupteur dans la ligne Settings et liste trois étapes : ajouter l’interrupteur Settings, utiliser le theme hook existant, et ajouter un test des réglages.](/blog/planning-with-ai-coding-agents/plan.png "Le plan nomme l’approche et les étapes avant qu’aucun code ne soit écrit.")

Le même ticket propose trois dispositions à comparer. L’agent PM en recommande une, et en choisir une autre met le plan à jour :

![Options de conception pour l’interrupteur de mode sombre : A, un interrupteur dans la ligne Settings, marqué Recommended ; B, les boutons Light, Dark et System, montrés dans un aperçu en direct ; et C, des cartes d’aperçu du thème.](/blog/planning-with-ai-coding-agents/design-options.png "Comparez les dispositions avant que l’agent en construise une.")

[Concepts](/docs/concepts) explique chaque étape que traverse un ticket.

## Lequel choisir

- **Une petite tâche dans une seule session :** le mode plan.
- **Une tâche plus grande, sur plusieurs sessions ou plusieurs agents :** un fichier de plan.
- **Beaucoup de tâches, avec le plan comme étape que vous approuvez avant tout code :** Coppice. Commencez par [Premiers pas](/docs/getting-started).
