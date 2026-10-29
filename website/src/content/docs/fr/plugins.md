---
title: Plugins
metaTitle: "Plugins : skills et outils MCP pour vos agents"
description: "Les plugins donnent aux agents Coppice des skills et des outils MCP. Installez-en un depuis git ou un dossier, activez-le et choisissez ses agents."
---

# Plugins

Les plugins donnent aux agents des skills et des outils MCP. Ajoutez-en un, activez-le, et choisissez quels agents peuvent s’en servir.

Un plugin est un dossier. Il peut contenir des skills (fichiers `SKILL.md`) et des outils MCP (`.mcp.json`). Ouvrez **Plugins** dans la barre latérale pour les gérer.

Le plugin intégré `coppice` est toujours là. Ses skills ne peuvent pas être désactivées.

<a id="install-a-plugin"></a>

## Installer un plugin
Ouvrez l’onglet **Custom**. Il y a deux façons de faire.

<a id="from-git"></a>

### Depuis git
1. Collez la **Git URL**.
2. Facultatif : réglez **Ref (optional)**, par exemple `main`.
3. Choisissez un **Target directory**.
4. Cliquez sur **Install**.

Coppice clone le dépôt avec les identifiants git de votre ordinateur. Cliquez plus tard sur **Update** sur la carte pour récupérer les nouveautés.

<a id="from-a-folder"></a>

### Depuis un dossier
1. Sous **Plugin directories**, cliquez sur **Browse…** et choisissez un dossier. Ou saisissez-le dans **Directory path**.
2. Cliquez sur **Add directory**.

Le dossier peut être un seul plugin, ou contenir plusieurs plugins en sous-dossiers. Cliquez sur **Rescan** après avoir modifié des fichiers.

Si deux dossiers ont un plugin du même nom, celui qui est le plus haut dans la liste l’emporte. Utilisez les flèches pour réordonner les dossiers. Votre dossier par défaut affiche **Default** et ne peut pas être retiré.

<a id="enable-it"></a>

## L’activer
Les plugins démarrent désactivés.

1. Activez l’interrupteur sur la carte du plugin. Seuls les plugins avec le statut `ok` peuvent être activés.
2. Si la carte a des **Settings**, remplissez chaque valeur et cliquez sur **Save settings**. Les valeurs sont stockées chiffrées.
3. Cliquez sur **Test** pour démarrer les outils du plugin et les lister. Ça marche avant même de l’activer.

Un plugin avec un serveur MCP local tourne sur votre ordinateur, avec les privilèges de votre ordinateur. Coppice vous demande de confirmer d’abord.

Chaque skill a son propre interrupteur. Une skill désactivée est cachée à tous les agents, même pendant des lancements déjà en cours.

<a id="choose-which-agents-can-use-it"></a>

## Choisir quels agents peuvent s’en servir
Sur la carte, sous **Which agents can use it?**, choisissez :

- **All agents** : tous les agents, y compris ceux que vous créerez plus tard.
- **Choose agents** : seulement les agents que vous cochez.

Vous pouvez aussi choisir les plugins dans le formulaire d’un agent, sous **Skills**. Les deux endroits restent synchronisés. Les changements s’appliquent au prochain lancement de chaque agent.

Les nouveaux plugins démarrent ainsi :

- Skills seulement : **All agents**.
- Avec un serveur MCP : **Choose agents**, sans personne de coché.

<a id="use-it-on-a-ticket"></a>

## L’utiliser sur un ticket
Activez le plugin et donnez-y accès au responsable du ticket. Cliquez ensuite sur **Run agent**.

L’agent voit le nom et la description de chaque skill, et charge une skill quand il en a besoin. Les outils s’appellent `<plugin>__<tool>`.

Pour voir ce qui a servi, ouvrez l’onglet **Agent Runs** du ticket, dépliez un lancement et ouvrez **Tools & Skills**. Les appels de plugin sont marqués **Plugin**.

<a id="create-a-plugin"></a>

## Créer un plugin
Le plugin le plus simple est un dossier avec un `SKILL.md`. Pour un nom, une version et des outils MCP, ajoutez un manifeste.

```
hello-coppice/
├── .claude-plugin/
│   └── plugin.json
├── skills/
│   └── greeting-style/
│       └── SKILL.md
└── .mcp.json
```

### `plugin.json`
Placez-le dans `.claude-plugin/plugin.json`.

```json
{
  "name": "hello-coppice",
  "version": "0.1.0",
  "description": "Example plugin: one skill and one MCP tool that greets people.",
  "author": { "name": "Coppice" }
}
```

| Champ | Obligatoire | Notes |
| --- | --- | --- |
| `name` | Oui | Lettres, chiffres, `.`, `_` et `-`, jusqu’à 64 caractères. Commence par une lettre ou un chiffre. `coppice` est réservé. |
| `version` | Non | `0.0.0` par défaut. |
| `description` | Non | Vide par défaut. |
| `author` | Non | Une chaîne, ou `{ "name": "..." }`. |
| `skills` | Non | Un chemin vers un dossier de skill, ou une liste de chemins de dossiers, relatifs à la racine du plugin. |

Sans `plugin.json`, un dossier reste un plugin s’il a un `SKILL.md` à la racine, ou des skills dans `skills/`, `.agents/skills/`, `.claude/skills/` ou `<name>/SKILL.md`. Il prend le nom du dossier et la version `0.0.0`.

<a id="skills"></a>

### Skills
Chaque skill est un dossier avec un `SKILL.md`. Le nom du dossier est le nom de la skill. Le fichier commence par un frontmatter :

```yaml
---
name: greeting-style
description: How to greet people in ticket comments. Load before writing a greeting.
---
```

`name` et `description` sont obligatoires. Rédigez la description pour que l’agent sache quand charger la skill.

<a id="mcp-tools"></a>

### Outils MCP
Ajoutez un `.mcp.json` avec une table `mcpServers`. Chaque serveur est local ou distant.

- Local : `command`, plus `args` et `env` facultatifs.
- Distant : `"type": "http"`, `url`, plus `headers` facultatif.

```json
{
  "mcpServers": {
    "hello": {
      "command": "node",
      "args": ["${CLAUDE_PLUGIN_ROOT}/server/index.mjs"],
      "env": { "HELLO_GREETING": "${HELLO_GREETING:-Hello}" }
    }
  }
}
```

`${NAME}` devient un réglage sur la carte du plugin. `${NAME:-default}` lui donne une valeur par défaut. Coppice remplace `${CLAUDE_PLUGIN_ROOT}` par le dossier d’installation du plugin. Ce n’est pas un réglage de la carte. Les serveurs SSE ne sont pas pris en charge.

Cet exemple expose l’outil `hello-coppice__greet` et la skill `hello-coppice:greeting-style`. Voir l’exemple complet dans [`examples/plugins`](https://github.com/fertile-org/coppice/tree/main/examples/plugins).

Les dossiers `commands/`, `agents/` et `hooks/` ne sont pas encore pris en charge. Ils affichent « Not supported yet » et ne s’exécutent pas.
