---
title: Plugins
metaTitle: "Plugins : skills et outils MCP pour vos agents"
description: "Les plugins donnent aux agents Coppice des skills et des outils MCP. Installez-en un depuis git ou un dossier, activez-le et choisissez ses agents."
---

# Plugins

Les plugins donnent aux agents des skills et des outils MCP. Un plugin est un dossier qui peut contenir :

- **Des skills** : des fichiers `SKILL.md`.
- **Des outils MCP** : un fichier `.mcp.json`.

Ouvrez **Plugins** dans la barre latérale pour les gérer. Le plugin intégré `coppice` est toujours là, et ses skills ne peuvent pas être désactivées.

<a id="install-a-plugin"></a>

## 1. Installer un plugin
La page **Plugins** a trois onglets :

- **Installed** : tous vos plugins, quelle que soit leur origine.
- **Marketplace** : le moyen le plus rapide d’en ajouter un.
- **Custom** : installer depuis git ou un dossier.

<a id="from-the-marketplace"></a>

### Depuis la marketplace
1. Ouvrez **Plugins** et passez à l’onglet **Marketplace**.
2. Cherchez, ou filtrez par source ou par catégorie.
3. Cliquez sur **Install** sur une entrée.

<figure>
<img src="/assets/docs/plugin-marketplace.webp" width="1440" height="900" alt="L’onglet Marketplace de la page Plugins, avec la recherche, les filtres par source et des cartes de plugins avec un bouton Install.">
<figcaption>Cherchez parmi des milliers de plugins, et installez-en un en un clic.</figcaption>
</figure>

Le plugin arrive désactivé dans l’onglet **Installed**. Ensuite, [activez-le](#enable-it). Un plugin fonctionne pour tous les agents à qui vous le donnez, quelle que soit leur CLI.

<a id="marketplace-sources"></a>

#### Sources
Cinq sources sont listées d’emblée :

- **coppice** : le catalogue fourni avec Coppice.
- **claude-plugins-official** : le répertoire officiel d’Anthropic.
- **claude-community** : le répertoire communautaire d’Anthropic.
- **superpowers-marketplace**
- **ponytail**

Comment les sources se chargent :

- Chaque source se charge seule la première fois que vous ouvrez l’onglet **Marketplace**.
- Chaque source se recharge en arrière-plan si elle a plus de 24 heures.
- Une pastille de source en échec affiche une erreur. Cliquez dessus pour réessayer.

<a id="community-plugins"></a>

#### Qui a relu un plugin
- Les cartes marquées **Coppice** viennent du catalogue Coppice.
- Toutes les autres sont marquées **Community: not reviewed by Coppice**. Relisez-les avant de les activer.
- Une entrée de catalogue qui fixe un `sha` installe exactement ce commit, même si la branche a bougé.

<a id="manage-sources"></a>

#### Gérer les sources
Cliquez sur **Manage sources** pour voir chaque source et son dernier rafraîchissement.

- **Ajouter une source** : ajoutez un dépôt git qui contient un `.claude-plugin/marketplace.json`. Coppice récupère aussitôt son catalogue.
- **Récupérer à nouveau** : cliquez sur **Refresh**, ou sur **Retry** après une erreur.
- **Retirer une source** : vous pouvez retirer toutes les sources sauf **coppice**. Les plugins installés depuis cette source restent installés. Une source par défaut retirée ne revient pas.

<a id="example-github"></a>

#### Exemple : GitHub
1. Installez `github` depuis la marketplace.
2. Ouvrez-le, collez un jeton d’accès personnel GitHub dans `GITHUB_PERSONAL_ACCESS_TOKEN` sous **Settings**, puis cliquez sur **Save settings**.
3. Cliquez sur **Test** pour lister ses outils.
4. Activez-le et choisissez les agents qui peuvent s’en servir.

Ce jeton est distinct du jeton GitHub dans **Settings** que Coppice utilise pour les pull requests.

<a id="from-git"></a>

### Depuis git
1. Ouvrez l’onglet **Custom**.
2. Collez la **Git URL**.
3. Facultatif : réglez **Ref (optional)**, par exemple `main`.
4. Choisissez un **Target directory**.
5. Cliquez sur **Install**.

Coppice clone le dépôt avec les identifiants git de votre ordinateur. Cliquez plus tard sur **Update** dans ses détails pour récupérer les nouveautés.

<a id="from-a-folder"></a>

### Depuis un dossier
1. Ouvrez l’onglet **Custom**.
2. Sous **Plugin directories**, cliquez sur **Browse…** et choisissez un dossier. Ou saisissez-le dans **Directory path**.
3. Cliquez sur **Add directory**.

- Le dossier peut être un seul plugin, ou contenir plusieurs plugins en sous-dossiers.
- Cliquez sur **Rescan** après avoir modifié des fichiers.
- Si deux dossiers ont un plugin du même nom, le plugin le plus haut dans la liste l’emporte. Utilisez les flèches pour réordonner les dossiers.
- Votre dossier par défaut affiche **Default** et ne peut pas être retiré.

<a id="enable-it"></a>

## 2. L’activer
Les plugins démarrent désactivés.

1. Activez l’interrupteur sur la ligne du plugin. Seuls les plugins avec le statut `ok` peuvent être activés.
2. Si ses détails ont des **Settings**, remplissez chaque valeur et cliquez sur **Save settings**. Les valeurs sont stockées chiffrées.
3. Cliquez sur **Test** pour démarrer les outils du plugin et les lister. Le test fonctionne avant même l’activation du plugin.

- Un plugin avec un serveur MCP local tourne sur votre ordinateur, avec les privilèges de votre ordinateur. Coppice vous demande de confirmer d’abord.
- Chaque skill a son propre interrupteur. Une skill désactivée est cachée à tous les agents, même pendant des lancements déjà en cours.

<a id="choose-which-agents-can-use-it"></a>

## 3. Choisir quels agents peuvent s’en servir
Dans ses détails, sous **Which agents can use it?**, choisissez :

- **All agents** : tous les agents, y compris ceux que vous créerez plus tard.
- **Choose agents** : seulement les agents que vous cochez.

Les nouveaux plugins démarrent ainsi :

- Skills seulement : **All agents**.
- Avec un serveur MCP : **Choose agents**, sans aucun agent coché.

- Vous pouvez aussi choisir les plugins dans le formulaire d’un agent, sous **Skills**. Les deux endroits restent synchronisés.
- Les changements s’appliquent au prochain lancement de chaque agent.

<a id="use-it-on-a-ticket"></a>

## 4. L’utiliser sur un ticket
1. Activez le plugin.
2. Donnez-y accès au responsable du ticket.
3. Cliquez sur **Run agent**.

- L’agent voit le nom et la description de chaque skill, et charge une skill quand il en a besoin.
- Les outils s’appellent `<plugin>__<tool>`.

Pour voir ce qui a servi :

1. Ouvrez l’onglet **Agent Runs** du ticket.
2. Dépliez un lancement.
3. Ouvrez **Tools & Skills**. Les appels de plugin sont marqués **Plugin**.

<a id="create-a-plugin"></a>

## 5. Créer un plugin
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

Sans `plugin.json`, un dossier reste un plugin s’il a :

- Un `SKILL.md` à la racine.
- Ou des skills dans `skills/`, `.agents/skills/`, `.claude/skills/` ou `<name>/SKILL.md`.

Ce plugin prend alors le nom du dossier et la version `0.0.0`.

<a id="skills"></a>

### Skills
Chaque skill est un dossier avec un `SKILL.md`. Le nom du dossier est le nom de la skill. Le fichier commence par un frontmatter :

```yaml
---
name: greeting-style
description: How to greet people in ticket comments. Load before writing a greeting.
---
```

- `name` et `description` sont obligatoires.
- Rédigez la description pour que l’agent sache quand charger la skill.

<a id="mcp-tools"></a>

### Outils MCP
Ajoutez un `.mcp.json` avec une table `mcpServers`. Chaque serveur est local ou distant :

- Local : `command`, plus `args` et `env` facultatifs.
- Distant : `"type": "http"`, `url`, plus `headers` facultatif.

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

- `${NAME}` devient un réglage dans les détails du plugin.
- `${NAME:-default}` donne à ce réglage une valeur par défaut.
- `${CLAUDE_PLUGIN_ROOT}` est remplacé par Coppice par le dossier d’installation du plugin. Ce n’est pas un réglage.
- Les serveurs SSE ne sont pas pris en charge.

Cet exemple expose l’outil `hello-coppice__greet` et la skill `hello-coppice:greeting-style`. Voir l’exemple complet dans [`examples/plugins`](https://github.com/fertile-org/coppice/tree/main/examples/plugins).

Les dossiers `commands/`, `agents/` et `hooks/` ne sont pas encore pris en charge. Ils affichent « Not supported yet » et ne s’exécutent pas.
