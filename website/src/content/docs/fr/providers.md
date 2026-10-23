---
title: Fournisseurs
description: "Les CLI d’agents que Coppice pilote : Claude Code, Codex, Cursor, OpenCode, Kilo Code, GitHub Copilot, Qwen Code, Kimi Code et Grok."
---

# Fournisseurs

Fatigué de garder un terminal ouvert pour chaque agent ? Coppice pilote les CLI d’agents de code que vous utilisez déjà et réunit leur travail sur un seul tableau. Vous gardez vos abonnements et vos connexions. Pas de compte Coppice.

<a id="supported-connectors"></a>

## Connecteurs pris en charge

| Connecteur | Commande que Coppice cherche | Installation | Versions testées |
| --- | --- | --- | --- |
| Claude Code | `claude` | [Guide d’installation](https://docs.anthropic.com/en/docs/claude-code/setup) | 2.1.309 |
| Codex | `codex` | [Guide d’installation](https://developers.openai.com/codex/cli) | 0.163.0 |
| Cursor | `cursor-agent` | [Guide d’installation](https://cursor.com/docs/cli/installation) | 2026.10.01 |
| OpenCode | `opencode` | [Guide d’installation](https://opencode.ai/docs/) | 1.x, 2.x |
| Kilo Code | `kilo` | [Guide d’installation](https://kilo.ai/docs/cli) | 7.8.3 |
| GitHub Copilot | `copilot` | [Guide d’installation](https://docs.github.com/en/copilot/how-tos/copilot-cli) | 1.0.98 |
| Qwen Code | `qwen` | [Guide d’installation](https://github.com/QwenLM/qwen-code) | 0.25.2 |
| Kimi Code | `kimi` | [Guide d’installation](https://moonshotai.github.io/kimi-code/) | 2.2.0 |
| Grok | `grok` | [Guide d’installation](https://x.ai/cli) | 1.0.62 |

Les informations de compatibilité seront mises à jour au fil du temps, à mesure que nous testons d’autres versions des CLI.

<figure>
<img src="/assets/docs/model-picker.webp" width="1440" height="900" alt="Le sélecteur de modèle d’un agent, avec les modèles de la CLI de l’agent.">
<figcaption>Choisissez un modèle, ou laissez celui par défaut.</figcaption>
</figure>

Choisissez un modèle pour cet agent, ou laissez celui par défaut. Vous avez installé un nouveau modèle ? Cliquez sur Refresh models.

<a id="connect-your-agent-cli"></a>

## Connecter votre CLI d’agent

1. Installez la CLI et connectez-vous dans votre terminal, comme d’habitude.
2. Choisissez votre CLI d’agent en créant un agent, et Coppice l’active.

Le formulaire d’agent liste chaque connecteur avec son statut :

| Statut | Ce que ça veut dire |
| --- | --- |
| **Ready** | Une vérification de connexion gratuite a confirmé que vous êtes connecté. |
| **Found, not signed in** | Coppice a trouvé la commande, mais vous n’êtes pas connecté. Connectez-vous depuis votre terminal. |
| **Not on your PATH** | Coppice ne trouve pas la commande. Installez-la, ou vérifiez votre PATH. |
| **Installed** | Coppice a trouvé la commande. Pour Kilo Code, GitHub Copilot, Qwen Code et Kimi Code, il ne peut pas vérifier la connexion sans utiliser votre forfait. |

Kilo Code, GitHub Copilot, Qwen Code et Kimi Code affichent **Installed**. System → Connectors ajoute : « Coppice can't check this sign-in without using your plan. A run will show an error if you're signed out. » Pour vérifier que l’une fonctionne, utilisez **Test connection** : il lance une courte conversation, un plan et un ticket. Toutes les CLI peuvent discuter. Les conversations Claude Code, Codex, Cursor et Kilo Code affichent « Coppice hasn't confirmed that {cli} stays read-only. The agent can still run commands on your computer. » Les conversations OpenCode, GitHub Copilot, Qwen Code, Kimi Code et Grok n’ont pas de bandeau : dans les conversations et la planification, Coppice bloque leurs écritures de fichiers et leurs commandes shell. Un outil MCP que vous avez configuré vous-même dans cette CLI peut encore modifier des choses.

Vous pouvez enregistrer un agent avant que sa CLI soit prête. Il commence à travailler sur les tickets une fois la CLI installée et connectée. Si vous avez installé la CLI après avoir ouvert Coppice, quittez et rouvrez Coppice pour qu’il trouve la nouvelle commande.

Un connecteur dont Coppice trouve la commande est activé, sauf si vous n’êtes pas connecté. Pour en activer ou en désactiver un, utilisez son interrupteur dans **System → Connectors**.

<a id="use-your-own-api-key-or-another-provider"></a>

## Utiliser votre propre clé d’API ou un autre fournisseur

Vous voulez utiliser votre propre clé d’API, ou un modèle que votre CLI ne propose pas ? Utilisez OpenCode. Il fonctionne avec plus de 75 fournisseurs de modèles et avec des modèles locaux. Voir [la liste des fournisseurs d’OpenCode](https://opencode.ai/docs/providers/).

1. Installez OpenCode.
2. Lancez `opencode` dans votre terminal, tapez `/connect`, choisissez votre fournisseur et collez votre clé. OpenCode garde la clé, et Coppice ne la demande jamais. Pas dans la liste ? Suivez les [étapes pour un fournisseur personnalisé](https://opencode.ai/docs/providers/#custom-provider) d’OpenCode.
3. Dans Coppice, créez un agent et réglez **Connector** sur **OpenCode**. Les modèles de votre fournisseur apparaissent sous **Model**. Si vous avez ajouté le fournisseur pendant que Coppice était ouvert, cliquez sur **Refresh models**.

<a id="on-a-ticket"></a>

## Sur un ticket

Quand un agent prend un ticket, Coppice lance sa CLI dans le worktree git propre à ce ticket. Vous pouvez suivre sa progression en direct sur le ticket.

<a id="what-coppice-does-not-do"></a>

## Ce que Coppice ne fait pas
- Il ne fait pas tourner de service de code dans le cloud
- Il ne fusionne pas de code sans votre Human Review
