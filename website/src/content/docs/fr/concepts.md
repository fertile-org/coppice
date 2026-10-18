---
title: Concepts
description: "Comment un ticket passe de Backlog à Done, et qui fait quoi."
---

# Concepts

Coppice est un tableau pour le travail de code. Vous écrivez les tickets, les agents travaillent chacun sur sa branche, et vous relisez chaque changement avant qu’il soit intégré. Rien ne démarre tant que vous n’agissez pas, et rien n’est fusionné tant que vous ne le décidez pas.

<a id="how-a-ticket-moves"></a>

## Le parcours d’un ticket

1. **Backlog** (à faire). Vous créez un ticket, ou vous approuvez celui qu’un agent a proposé.
2. **Plan Review** (revue du plan). Appuyez sur **Run agent**, et votre agent PM écrit un plan. Lisez-le, puis approuvez-le ou commentez. Si votre ticket n’est pas clair, l’agent PM peut d’abord vous poser jusqu’à 3 courtes questions. La carte affiche « Waiting for your answers » (en attente de vos réponses). Répondez dans le ticket et cliquez sur **Send answers**, puis l’agent PM écrit le plan. La planification ne modifie jamais le code du ticket. Avec **Skip planning** (sauter la planification) activé, un ticket passe directement de Backlog à Ready. Pour un flux, une architecture ou un modèle de données, le plan peut inclure un petit schéma. Le plan écrit est toujours là aussi.
3. **Ready** (prêt). Une fois le plan approuvé, votre agent Technical Lead choisit qui fait le travail. S’il n’y a pas de Technical Lead ou que personne ne convient, le ticket reste dans Ready, sans responsable, avec un commentaire.
4. **In Progress, In Review, In QA** (en cours, en revue, en QA). Les agents construisent, relisent et testent le changement sur la branche du ticket. Votre checkout principal n’est pas touché.
5. **Wait for Human Review** (en attente de revue humaine). Vous ouvrez le diff, donnez vos retours en commentaires, et choisissez **Accept** (accepter) quand c’est bon. Si un agent ajoute un commit ensuite, le ticket repasse en In Review, puis revient vers vous.
6. **Done** (terminé). Accepter ne fusionne pas. Choisissez **Merge…** quand vous êtes prêt : ce que vous avez accepté est exactement ce qui est fusionné. Si des fichiers entrent en conflit, Coppice ne touche pas à main et renvoie le ticket en In Progress, où le même agent résout les conflits. Vous l’acceptez de nouveau avant la fusion.

Un ticket bloqué passe en **Blocked** (bloqué) avec un commentaire qui explique pourquoi.

<a id="agents"></a>

## Agents

Coppice pilote les CLI d’agents que vous utilisez déjà, comme Claude Code, Codex, Cursor, OpenCode et Kilo Code. Chaque agent a un rôle, comme PM, Technical Lead ou Backend Engineer, et tourne sur une CLI. Voir [Fournisseurs](/docs/providers).

Chaque agent peut utiliser un modèle différent, même sur la même CLI. Votre PM peut tourner sur Claude Opus pendant que votre Backend Engineer tourne sur Claude Sonnet.

<a id="chat"></a>

## Chat

Ouvrez **Chat** et cliquez sur un agent pour parler de votre code, d’une idée ou d’un ticket. Chaque agent voit tous vos dépôts, tableaux, tickets et votre Knowledge. Il peut rédiger un ticket ou proposer une note Knowledge, mais rien n’est enregistré tant que vous ne l’avez pas approuvé. Choisissez **New chat** pour démarrer une conversation séparée.

Toutes les CLI peuvent discuter. Coppice n’a pas confirmé que Claude Code, Codex, Cursor ou Kilo Code restent en lecture seule, donc leurs conversations affichent un bandeau. L’agent peut quand même lancer des commandes sur votre ordinateur.

<a id="everything-is-on-the-card"></a>

## Tout est sur la carte

Commentaires, plans et lancements d’agents vivent tous sur le ticket. Les agents ne se parlent qu’à travers les commentaires du ticket : si quelque chose s’est passé, vous pouvez le lire là. Les agents peuvent ouvrir les fichiers que vous joignez à la description ou à un commentaire.

<a id="knowledge"></a>

## Knowledge

Les agents peuvent garder des notes sur votre projet pour les tickets suivants. Par défaut, c’est vous qui approuvez ce qu’ils conservent.
