---
title: Raccourcis clavier
description: "Se déplacer dans Coppice au clavier : le tableau, les tickets et la revue."
---

# Raccourcis clavier

Appuyez sur `?` n’importe où pour voir tous les raccourcis. Ils sont regroupés en **Navigate**, **Tickets** et **Review**. Modifiez les raccourcis dans Settings.

`Mod` correspond à `⌘` sur macOS et à `Ctrl` sur Linux et WSL. Les séquences s’affichent comme `G then B`.

Les raccourcis ne se déclenchent pas pendant que vous tapez dans un champ, et le terminal garde ses propres touches. Appuyez sur `Esc` pour fermer ce qui est ouvert.

<a id="navigate"></a>

## Naviguer (Navigate)

| Action | Id | Touches |
| --- | --- | --- |
| Ouvrir la palette de commandes | `palette.open` | `Mod+K` |
| Afficher les raccourcis clavier | `help.show` | `?` |
| Fermer | `close` | `Esc` |
| Carte suivante | `board.nextCard` | `J` or `Down` |
| Carte précédente | `board.previousCard` | `K` or `Up` |
| Colonne suivante | `board.nextColumn` | `L` or `Right` |
| Colonne précédente | `board.previousColumn` | `H` or `Left` |
| Aller à la colonne 1 | `board.column.1` | `G then 1` |
| Aller à la colonne 2 | `board.column.2` | `G then 2` |
| Aller à la colonne 3 | `board.column.3` | `G then 3` |
| Aller à la colonne 4 | `board.column.4` | `G then 4` |
| Aller à la colonne 5 | `board.column.5` | `G then 5` |
| Aller à la colonne 6 | `board.column.6` | `G then 6` |
| Aller à la colonne 7 | `board.column.7` | `G then 7` |
| Aller à la colonne 8 | `board.column.8` | `G then 8` |
| Aller à la colonne 9 | `board.column.9` | `G then 9` |
| Aller à Boards | `nav.boards` | `G then B` |
| Aller à Agents | `nav.agents` | `G then A` |
| Aller à Chat | `nav.chat` | `G then C` |
| Aller à Knowledge | `nav.knowledge` | `G then K` |
| Aller à Repositories | `nav.repositories` | `G then R` |
| Aller à Plugins | `nav.plugins` | `G then P` |
| Aller à Users | `nav.users` | `G then U` |
| Aller à System | `nav.tools` | `G then T` |
| Aller à Settings | `nav.settings` | `G then S` |
| Section suivante | `drawer.nextSection` | `]` |
| Section précédente | `drawer.previousSection` | `[` |

**Aller à la colonne 1** à **Aller à la colonne 9** suivent le tableau de gauche à droite : 1 Backlog, 2 Plan Review, 3 Ready, 4 In Progress, 5 In Review, 6 In QA, 7 Wait for Human Review, 8 Done, 9 Blocked.

**Aller à Plugins**, **Aller à Users**, **Aller à System** et **Aller à Settings** n’existent pas pour les membres, comme dans la barre latérale. L’application de bureau n’a pas non plus **Aller à Users**.

**Carte suivante** et **Carte précédente** se déplacent dans une colonne et ne bouclent pas. **Colonne suivante** et **Colonne précédente** arrivent sur la carte au même rang, sur la dernière carte, ou sur le titre de la colonne quand elle est vide. **Section suivante** et **Section précédente** font passer le panneau ouvert entre Detail, Live Console et Agent Runs.

<a id="tickets"></a>

## Tickets

| Action | Id | Touches | Quand |
| --- | --- | --- | --- |
| Ouvrir le ticket | `ticket.open` | `Enter` | Une carte a le focus |
| Créer un ticket | `ticket.create` | `C` | Tableau, aucun panneau ouvert |
| Assigner le ticket | `ticket.assign` | `A` | Une carte a le focus ou le panneau est ouvert |
| Passer à la colonne suivante | `ticket.moveNext` | `M then L` | Une carte a le focus ou le panneau est ouvert |
| Passer à la colonne précédente | `ticket.movePrevious` | `M then H` | Une carte a le focus ou le panneau est ouvert |
| Publier le commentaire | `ticket.comment` | `Mod+Enter` | L’éditeur de commentaire a le focus et contient du texte |

**Créer un ticket** met le focus sur le champ **Add ticket…** de la colonne active. Entrée envoie le formulaire. Ce champ est dans Backlog.

**Assigner le ticket** ouvre la liste des responsables. Si le panneau est fermé, le ticket s’ouvre puis la liste s’ouvre.

**Passer à la colonne suivante** et **Passer à la colonne précédente** suivent l’ordre des colonnes ci-dessus.

<a id="review"></a>

## Revue (Review)

| Action | Id | Touches | Quand |
| --- | --- | --- | --- |
| Approuver le plan | `review.approvePlan` | `R then P` | Le ticket est en Plan Review et **Approve plan** est actif |
| Accept… | `review.accept` | `Mod+Shift+Enter` | Le ticket est en Wait for Human Review et n’est pas archivé |
| Merge… | `review.merge` | `R then M` | **Merge…** est déjà affiché |
| Ouvrir la revue de code | `review.openCode` | `D` | **Open code review** est actif |

**Accept…** ouvre la boîte de dialogue **Accept this ticket?** avec le focus sur **Cancel**. Choisissez **Accept** pour terminer.

**Merge…** ouvre la boîte de dialogue **Merge ticket branch**.

<a id="a-pass-with-only-the-keyboard"></a>

## Un tour complet au clavier

1. Sur le tableau, appuyez sur `C`. Le focus va sur **Add ticket…**. Saisissez un titre et appuyez sur Entrée.
2. Allez sur cette carte avec `J` ou `K`, puis appuyez sur `A`. La liste des responsables s’ouvre.
3. Appuyez sur Entrée.
4. Mettez le focus sur l’éditeur de commentaire, tapez, puis appuyez sur `Mod+Enter`.
5. Appuyez sur `D`.
6. Quand le ticket est en Wait for Human Review, appuyez sur `Mod+Shift+Enter`. Le focus est sur **Cancel**. Choisissez **Accept** pour terminer.
7. Appuyez sur `Esc`. Le focus revient sur la carte du tableau.

<a id="custom-shortcuts"></a>

## Raccourcis personnalisés

Modifiez n’importe quel raccourci dans Settings, sous `[keybindings]`. Utilisez l’id de la commande donné dans les tableaux ci-dessus. Vos touches remplacent celles par défaut, et une valeur vide désactive le raccourci.

Si un raccourci est illisible, désigne une commande inconnue, utilise une touche déjà prise par une autre commande, ou appartient au navigateur ou au système, cette commande garde sa valeur par défaut. Settings explique pourquoi. L’aide des raccourcis et la palette de commandes affichent les touches réellement en vigueur.

```toml
[keybindings]
"palette.open" = "P"
"ticket.create" = ""
```

Ouvrez **Settings**, modifiez le fichier et choisissez **Save**. Les nouvelles touches apparaissent dans l’aide des raccourcis et la palette de commandes sans redémarrage.
