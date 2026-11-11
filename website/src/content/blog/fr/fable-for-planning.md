---
title: Comment planifier avec Fable et coder avec un modèle moins cher
metaTitle: Planifier avec Fable, coder moins cher
description: "Planifiez avec Fable 5.2 et codez avec Opus ou Sonnet. Claude Code n’a pas de réglage Fable puis modèle moins cher. Coppice peut fixer un modèle par agent."
answer: "Utilisez Fable 5.2 pour le plan et un modèle moins cher, Opus ou Sonnet, pour le code. Claude Code n’a pas ce réglage : opusplan reste Opus puis Sonnet. Planifiez avec /model fable puis changez de modèle, ou donnez son modèle à un subagent. Dans Coppice, chaque agent a son modèle : le PM peut planifier avec Fable et l’ingénieur coder avec Opus ou Sonnet."
published: "2026-11-11"
draft: false
author: Coppice
authorType: Organization
keywords:
  - Fable
  - Opus
  - Sonnet
  - Claude Code
  - planification
---

## Pourquoi c’est difficile

Claude Fable 5.2 est le modèle d’Anthropic pour le travail le plus difficile et le plus ouvert. Pour la plupart du code, Anthropic dit de commencer par Opus 5.5, et d’utiliser Sonnet 5.5 pour une tâche plus petite et mieux délimitée. Fable coûte plus cher qu’Opus, et Opus coûte plus cher que Sonnet. La plupart des tokens arrivent après le plan : lecture des fichiers, modifications et nouvelles tentatives. Si vous laissez Fable pour cette partie, il fait le travail mécanique. Vous voulez Fable sur le plan et Opus ou Sonnet sur le code, avec le plan encore là quand le code commence.

## Option 1 : planifier avec Fable, puis changer

Claude Code n’a pas de réglage qui utilise Fable pour le plan et un modèle moins cher pour le code. `opusplan` reste Opus en mode plan, puis Sonnet ensuite.

Exécutez `/model fable`, puis entrez en mode plan. Dans le terminal, cet alias est Fable 5.2. Dans une session de passerelle Claude apps, c’est encore Fable 5 : exécutez-y `/model claude-fable-5-2`. Quand le plan est prêt, exécutez `/model opus` ou `/model sonnet`. La conversation reste. Quand la session se termine, le plan disparaît.

## Option 2 : un subagent avec son propre modèle

Un subagent Claude Code est un fichier Markdown. Son champ `model` fixe le modèle de ce subagent : `fable`, `opus`, `sonnet`, `haiku`, ou un identifiant complet. Gardez la session sur Fable et confiez le code à Opus ou Sonnet. Enregistrez ceci sous `.claude/agents/` :

```markdown
---
name: implementer
description: Implements an approved plan
model: opus
---
```

Utilisez `sonnet` quand la tâche est petite. Seul le travail délégué tourne sur ce modèle. Le code que la session principale écrit elle-même tourne encore sur Fable.

## Option 3 : coller le plan dans une session moins chère

Planifiez avec Fable, copiez le plan, et recommencez sur Opus ou Sonnet :

```bash
claude --model opus
```

`claude --model sonnet` est la même étape pour une tâche plus petite. Collez le plan. Vous pouvez le coller de la même façon dans une autre CLI d’agent. La nouvelle session ne connaît que ce que vous avez collé.

## Option 4 : un modèle par agent, avec Coppice

[Coppice](/) est une application de bureau qui place vos agents de code sur un tableau de tâches. Chaque agent a son propre modèle, même sur la même CLI. Pour un ticket comme un interrupteur de mode sombre dans Settings, le PM peut planifier avec Fable 5.2 et le Frontend Engineer construire avec Opus 5.5 ou Sonnet.

1. [Installez Coppice](/docs/install). Connectez-vous à Claude Code dans le terminal comme d’habitude, et ajoutez votre projet.
2. Créez un agent PM, choisissez Claude Code, et réglez le modèle sur Fable 5.2.
3. Créez un Technical Lead et un Frontend Engineer sur Claude Code. Réglez le modèle du Frontend Engineer sur Opus 5.5, ou sur Sonnet quand le changement est petit.
4. Écrivez le ticket et lancez l’agent. L’agent PM écrit le plan avec Fable. Lisez-le, puis approuvez le plan ou demandez des changements. Cette exécution de planification ne modifie pas votre code.
5. Quand le code est prêt, relisez le diff.

Une fois que vous avez approuvé, le Technical Lead vérifie le plan et assigne le Frontend Engineer. L’exécution du Frontend Engineer utilise le modèle que vous avez réglé, et elle démarre d’elle-même. [Concepts](/docs/concepts) explique chaque étape.

![Fenêtre de nouvel agent nommé PM. La liste des modèles est ouverte, et Claude Fable 5.2 est sélectionné.](/blog/fable-for-planning/pm-fable.png "La liste des modèles du PM, avec Claude Fable 5.2 sélectionné.")

![Fenêtre de nouvel agent pour un Frontend Engineer. Claude Code est choisi et marqué Ready, et le modèle est Claude Opus 5.5.](/blog/fable-for-planning/frontend-opus.png "Le Frontend Engineer utilise Claude Code, avec Claude Opus 5.5 sélectionné.")

## Lequel choisir

- **Une seule session Claude Code, et le plan doit être sur Fable :** `/model fable`, puis `/model opus` ou `/model sonnet`. Utilisez `opusplan` quand Opus et Sonnet suffisent.
- **Un modèle moins cher pour une étape que vous déléguez :** un subagent avec `model: opus` ou `model: sonnet`.
- **Un plan à reprendre dans une nouvelle session :** collez-le dans une session sur Opus ou Sonnet.
- **Beaucoup de tickets, le plan sur Fable et le code sur Opus ou Sonnet :** Coppice. Commencez par [Premiers pas](/docs/getting-started).
