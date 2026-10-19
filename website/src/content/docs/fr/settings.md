---
title: Référence des paramètres
description: "Chaque champ modifiable de config.toml, avec son type et sa valeur par défaut."
---

# Référence des paramètres

Coppice garde ses paramètres dans un seul fichier, `config.toml` :

| Système | Chemin |
| --- | --- |
| Linux | `~/.config/Coppice/config.toml` |
| macOS | `~/Library/Application Support/Coppice/config.toml` |

Sous Linux, si `XDG_CONFIG_HOME` est défini, le fichier se trouve dans `$XDG_CONFIG_HOME/Coppice/config.toml`.

<a id="edit-it-in-the-app"></a>

## Le modifier dans l’application
Ouvrez **Settings** dans la barre latérale. La page affiche le fichier en texte, et vos commentaires sont conservés.

- **Save** vérifie le fichier avant de l’écrire. Une valeur incorrecte n’est pas enregistrée.
- **Restore last good version** annule une mauvaise modification.

Certains changements s’appliquent tout de suite. D’autres affichent « Saved. Restart Coppice to apply. » (enregistré ; redémarrez Coppice pour appliquer).

Vous pouvez aussi modifier le fichier dans n’importe quel éditeur de texte. Vos modifications survivent aux mises à jour.

<a id="updates"></a>

## Mises à jour
Ouvrez **Settings → Updates** pour voir votre version et choisir **Check for updates**. **Automatic updates** est activé par défaut. Désactivez-le, et Coppice arrête de chercher les nouvelles versions en arrière-plan.

Cet interrupteur n’est pas dans `config.toml`. Coppice l’enregistre sous `automaticUpdates` dans `update-settings.json`, dans le même dossier.

<a id="replace-keys"></a>

## Remplacer les clés

Si vous avez partagé une sauvegarde, créez de nouvelles clés et un nouveau mot de passe administrateur. Les jetons enregistrés continuent de fonctionner, et tout le monde est déconnecté.

Ouvrez **System → Backup → Replace keys**, saisissez le nouveau mot de passe administrateur, et confirmez.

<a id="logs"></a>

## Journaux
Les administrateurs peuvent ouvrir **System → Logs** pour filtrer et suivre en direct les journaux du serveur, des requêtes, des lancements d’agents et de la base de données. Chaque lancement a un lien **View logs**.

**Export logs** enregistre les lignes qui correspondent à vos filtres actuels dans `coppice-logs.zip`. Coppice retire les jetons et les clés qu’il connaît, mais vérifiez le fichier avant de le partager.

**Report a problem** ouvre un nouveau ticket GitHub avec votre version de Coppice déjà remplie. Dites ce qui s’est passé et ce que vous attendiez, puis joignez le zip d’**Export logs**.

## `[logs]`

| Champ | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `retention_days` | integer | `14` | Combien de jours de journaux garder, aujourd’hui compris : 3, 7, 14 ou 30. La valeur par défaut est 14. Coppice supprime les journaux plus anciens toutes les heures, et les changements s’appliquent sans redémarrage. Si les journaux dépassent 256 Mio, Coppice supprime d’abord les plus anciens, même avant la fin de `retention_days`. |

Le journal de chaque jour est découpé en fichiers de 32 Mio au plus, et chaque fichier compte dans la limite de 256 Mio, ceux du jour compris. Coppice ne supprime jamais le fichier de journal ni le journal de base de données en cours d’écriture. Les journaux de base de données comptent dans la limite, et Coppice les supprime une fois `retention_days` dépassé.

| Champ | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `dir` | string | `./data/logs` | Dossier des journaux. L’application de bureau utilise son propre dossier et ignore cette valeur. La changer demande un redémarrage. |

<a id="connectors"></a>

## Connecteurs
Les CLI connectées sont activées par défaut. Il ne faut une ligne que pour en désactiver une :

```toml
[agent.connectors.claude-code]
enabled = false
```

L’interrupteur dans **System → Connectors** écrit la même ligne.

Kilo Code, GitHub Copilot, Qwen Code et Kimi Code restent désactivés tant que vous ne les activez pas. Coppice ne peut pas vérifier leur connexion gratuitement.

## `[agent]`

| Champ | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `worktrees_path` | string | `<data-dir>/worktrees` | Dossier où chaque ticket a son propre worktree git. |
| `worker_count` | integer | `2` | Nombre de tâches d’agents exécutées en même temps. En dessous de 1, vaut 1. |
| `health_check_interval_secs` | integer | `60` | Secondes entre deux vérifications de santé des agents. En dessous de 10, vaut 10. |

`<data-dir>` est le dossier qui contient `config.toml` : `~/.config/Coppice` sous Linux et WSL, et `~/Library/Application Support/Coppice` sous macOS.

## `[agent.connectors.<id>]`

Ids : `claude-code`, `codex`, `cursor`, `opencode`, `kilo-code`. Chaque table est facultative.

| Champ | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `enabled` | bool | non défini : activé si la CLI est connectée | Active ou désactive le connecteur. |
| `model_providers` | list of strings | `[]` | Fournisseurs de modèles pour ce connecteur. Remplis quand vous l’activez. |
| `run_timeout_secs` | integer | `1800` pour OpenCode, `600` pour les autres | Secondes avant qu’un lancement sur ce connecteur échoue. |
| `command` | string | `agent` (Cursor), `codex`, `opencode`, `kilo` | Le binaire de CLI à lancer. Claude Code lance toujours `claude` et n’a pas de `command`. |

OpenCode uniquement :

| Champ | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `serve_hostname` | string | `127.0.0.1` | Nom d’hôte de l’assistant OpenCode pendant un lancement. Demande un redémarrage. |

Changer la `command` d’OpenCode demande aussi un redémarrage.

## `[workflow]`

| Champ | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `auto_start_runs` | bool | `false` | Si true, assigner un agent à un ticket qui a un dépôt lance un lancement. Si false, vous cliquez sur **Run agent**. |
| `auto_assign.default` | bool | `true` | Active l’assignation automatique pour chaque statut sans réglage propre. |
| `auto_assign.<status>` | bool | `backlog` : `false`. Autres : suit `default` | Active ou désactive l’assignation automatique pour un statut. |
| `auto_split.default` | bool | `false` | Active le découpage automatique pour chaque statut sans réglage propre. |
| `auto_split.<status>` | bool | suit `default` | Active ou désactive le découpage automatique pour un statut. |

Clés de statut : `backlog`, `ready`, `in_progress`, `in_review`, `in_qa`, `wait_for_final_review` (Wait for Human Review), `blocked`, `done`.

Quand l’assignation automatique est activée pour ce statut, Coppice assigne l’agent désigné par le lancement. Sinon, le ticket affiche **Recommends:** à la place.

Quand le découpage automatique est activé pour ce statut, Coppice crée les tickets enfants que le lancement propose. Sinon, il enregistre une **Pending split recommendation** jusqu’à ce que vous cliquiez sur **Approve splits**.

## `[knowledge]`

| Champ | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | Active ou désactive le compactage de Knowledge. |
| `poll_interval_ms` | integer | `500` | Fréquence de réveil du planificateur de compactage. En dessous de 100, vaut 100. |
| `compaction.interval_secs` | integer | `1800` | Secondes entre deux cycles de compactage. |
| `compaction.batch_max_tickets` | integer | `10` | Nombre maximal de tickets dans un lot. |
| `compaction.batch_max_source_bytes` | integer | `200000` | Nombre maximal d’octets de source dans un lot. |
| `compaction.max_candidates_per_batch` | integer | `20` | Nombre maximal de candidats qu’un lot peut proposer. |
| `compaction.max_attempts` | integer | `3` | Nombre maximal d’essais pour un lot. |
| `auto_save.enabled` | bool | `false` | Enregistre les candidats peu risqués sans vous demander. |
| `auto_save.allowed_types` | list of strings | `[]` | Types que l’enregistrement automatique peut garder : `bug_pattern`, `coding_convention`, `dependency_note`, `performance_note`, `review_feedback`, `test_command`. |
| `auto_save.minimum_confidence` | string | `high` | Doit rester `high`. |
| `retrieval.top_k` | integer | `8` | Nombre d’entrées renvoyées par une recherche. De 1 à 20. |
| `retrieval.allowed_types` | list of strings | `[]` (tous les types) | Ne renvoie que ces types : `coding_convention`, `architecture_rule`, `bug_pattern`, `test_command`, `review_feedback`, `dependency_note`, `api_contract`, `workflow_rule`, `human_preference`, `operational_runbook`, `security_rule`, `performance_note`. |
| `retrieval.minimum_confidence` | string | `medium` | `low`, `medium` ou `high`. |
| `retrieval.default_page_size` | integer | `25` | Taille de page par défaut. |
| `retrieval.max_page_size` | integer | `100` | Taille de page maximale. 100 au plus. |
| `retrieval.max_active_per_board` | integer | `1000` | Nombre maximal d’éléments Knowledge, en attente ou approuvés, qu’un tableau peut contenir. Les éléments des agents de ce tableau comptent aussi. |
| `retrieval.max_active_workspace` | integer | `10000` | Nombre maximal d’éléments Knowledge, en attente ou approuvés, qui s’appliquent à tout l’espace de travail. |

Chaque nombre et intervalle ci-dessus doit être supérieur à 0.

## `[git]`

| Champ | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `author_name` | string | `Coppice` | Nom d’auteur des commits créés par Coppice. |
| `author_email` | string | `coppice@localhost` | E-mail d’auteur de ces commits. |
| `push_enabled` | bool | `false` | Autorise Coppice à pousser des branches et à ouvrir des pull requests. |

## `[plugins]`

| Champ | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `dir` | string | `<data-dir>/plugins` | Votre dossier de plugins par défaut. Ajoutez d’autres dossiers sur la page Plugins. |
| `allow_file_git_urls` | bool | `false` | Autorise les URL git `file:` pour installer des plugins. |
| `git_timeout_secs` | integer | `300` | Secondes avant qu’un clonage ou une mise à jour de plugin échoue. |
| `mcp_start_timeout_secs` | integer | `20` | Secondes d’attente du démarrage du serveur MCP d’un plugin. |
| `mcp_list_timeout_secs` | integer | `10` | Secondes d’attente pour qu’un plugin liste ses outils. |
| `mcp_idle_shutdown_secs` | integer | `600` | Secondes pendant lesquelles un serveur MCP de plugin inactif reste lancé. |

## `[mcp]`

| Champ | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `builtin_plugins_dir` | string | `<data-dir>/builtin-plugins` | Où Coppice écrit ses skills intégrées à chaque démarrage. Les modifications faites là sont perdues. |
| `call_timeout_secs` | integer | `60` | Secondes avant qu’un appel d’outil échoue. |
| `max_output_bytes` | integer | `32768` | Taille maximale d’un résultat d’outil, en octets. |
| `comment_post_limit` | integer | `5` | Nombre maximal de commentaires qu’un lancement peut publier. |
| `token_ttl_secs` | integer | `14400` | Durée de vie du jeton d’outils d’un lancement, en secondes. |

## `[keybindings]`

Chaque clé est un id de commande de [Raccourcis clavier](/docs/shortcuts). Vos touches remplacent celles par défaut de la commande, et une valeur vide désactive le raccourci.

| Champ | Type | Défaut | Rôle |
| --- | --- | --- | --- |
| `"<command id>"` | string or list of strings | le raccourci intégré | Remplace les touches de cette commande. `""` les retire. |

```toml
[keybindings]
"palette.open" = "P"
"ticket.create" = ""
"board.nextCard" = ["J", "Down"]
```

`Mod` correspond à Cmd sur macOS et à Ctrl sur Linux et WSL. Une séquence s’écrit `G then B`. Plusieurs touches pour une même commande se séparent par ` or `, ou s’écrivent en liste. Save applique le changement sans redémarrage. Un raccourci que Coppice ne sait pas lire, un id de commande inconnu, une touche déjà prise par une autre commande, ou une touche qui appartient au navigateur ou au système est ignoré. La commande garde sa valeur par défaut, et Settings explique pourquoi.
