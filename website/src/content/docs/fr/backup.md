---
title: Sauvegarder les données
description: "Sauvegarder Coppice, le déplacer sur un nouvel ordinateur, et remplacer les clés si une sauvegarde fuit."
---

# Sauvegarder les données

<a id="download-a-backup"></a>

## Télécharger une sauvegarde

Ouvrez **Tools** (outils), puis **Backup** (sauvegarde), et choisissez **Download backup** (télécharger une sauvegarde). Vous obtenez un seul fichier avec vos tableaux, tickets, paramètres, fichiers et worktrees d’agents.

Les mots de passe, les clés et les jetons enregistrés n’y sont pas. La sauvegarde contient tout de même toutes les données de votre espace de travail, alors gardez-la pour vous.

<a id="import-a-backup"></a>

## Importer une sauvegarde

Sur l’ordinateur vers lequel vous déménagez, ouvrez **Tools**, puis **Backup**, et choisissez **Import backup** (importer une sauvegarde).

L’import remplace toutes les données de cet ordinateur par celles de la sauvegarde. Votre tableau et les paramètres de l’espace de travail suivent. Les mots de passe, les clés, les chemins de dossiers, les commandes des agents et les réglages des plugins et de MCP ne suivent pas : configurez-les de nouveau. Vous serez peut-être déconnecté.

Les jetons enregistrés ne sont pas dans la sauvegarde, il faut donc les recoller un par un ensuite. Coppice marque chaque jeton nécessaire avec « Paste this token again. » (recollez ce jeton). Collez-le, puis enregistrez.

<a id="copy-the-folders-by-hand"></a>

## Copier les dossiers à la main

Vous pouvez aussi quitter Coppice et copier son dossier de données :

| | Données | Journaux |
| --- | --- | --- |
| macOS | `~/Library/Application Support/Coppice` | `~/Library/Logs/Coppice` |
| Linux | `~/.config/Coppice` | `~/.local/state/Coppice/logs` |

Cette copie contient le dossier `secrets` : traitez-la comme un mot de passe. Vos connexions aux CLI n’y sont pas. Chaque CLI garde les siennes.

<a id="replace-keys"></a>

## Remplacer les clés

Si vous avez partagé une sauvegarde, ou si vous pensez que quelqu’un d’autre en a une copie, créez de nouvelles clés.

Ouvrez **Tools**, puis **Backup**, et choisissez **Replace keys** (remplacer les clés). Saisissez un nouveau mot de passe administrateur et cochez la case. Coppice crée de nouvelles clés et de nouveaux mots de passe de base de données. Les jetons enregistrés continuent de fonctionner, et tout le monde est déconnecté.
