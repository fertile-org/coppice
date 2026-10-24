---
title: FAQ
metaTitle: "FAQ Coppice : Beta, Docker, Windows et merge"
description: "Coppice est-il prêt pour la production ? Faut-il Docker ou un compte ? Les agents fusionnent-ils sans vous ? Réponses sur la Beta, Windows, les conflits, etc."
---

# FAQ

<a id="is-coppice-production-ready"></a>

## Coppice est-il prêt pour la production ?
Non, pas encore. Coppice est en beta, pour les premiers utilisateurs qui veulent un tableau d’agents local avec de vraies validations humaines. Les agents tournent avec vos permissions locales.

<a id="do-i-need-docker-or-an-account"></a>

## Faut-il Docker ou un compte ?
Non. Téléchargez l’application de bureau, ouvrez-la, et travaillez. Pas de Docker, pas de serveur à installer, pas de compte Coppice.

<a id="is-this-a-jira--linear-replacement"></a>

## Est-ce un remplaçant de Jira / Linear ?
Non. Coppice est un tableau pour piloter des agents de code, pas un outil de suivi de tickets pour toute l’entreprise.

<a id="is-this-a-cloud-coding-agent"></a>

## Est-ce un agent de code dans le cloud ?
Non. Les agents tournent via des CLI sur votre machine, sur vos dépôts. La différence : les services d’agents dans le cloud confient le travail à l’ordinateur de quelqu’un d’autre ; Coppice garde le travail en local, et c’est vous qui validez.

<a id="will-agents-merge-without-me"></a>

## Les agents vont-ils fusionner sans moi ?
Non. La Human Review est une étape de validation à part entière. Vous ouvrez le diff et vous décidez.

<a id="what-happens-if-a-tickets-changes-conflict-with-main"></a>

## Que se passe-t-il si les changements d’un ticket entrent en conflit avec main ?
Coppice s’arrête avant la fusion et ne touche pas à main. Il liste les fichiers en conflit et renvoie le ticket en In Progress, où le même agent commence à les résoudre. Vous l’acceptez ensuite de nouveau en Human Review avant la fusion.

<a id="what-about-set-and-forget-autopilots"></a>

## Et les pilotes automatiques « on lance et on oublie » ?
Ce n’est pas le produit Coppice. Coppice est fait pour gérer des agents : c’est vous qui lancez le travail, et vous acceptez chaque changement.

<a id="windows"></a>

## Et Windows ?
Windows 11 devrait pouvoir faire tourner la version Linux x64 via WSL, mais nous ne l’avons pas encore testé. Voir [Windows 11 (via WSL)](/docs/install#windows-11-through-wsl). Si vous essayez, [racontez-nous sur GitHub](https://github.com/fertile-org/coppice/issues). Aucune application Windows native n’est prévue pour cette beta.

<a id="where-do-contributors-look"></a>

## Où regardent les contributeurs ?
La documentation d’architecture, de développement et de tests reste dans le [dépôt GitHub](https://github.com/fertile-org/coppice/tree/main/docs). Ce site sert à utiliser Coppice.
