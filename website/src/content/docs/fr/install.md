---
title: Installer
description: "Coppice Beta est une application de bureau. Pas de Docker, pas de serveur, pas de compte Coppice."
---

# Installer

Coppice Beta est une application de bureau. Pas de Docker, pas de serveur, pas de compte Coppice.

<a id="supported-platforms"></a>

## Plateformes prises en charge
| Plateforme | Paquet |
| --- | --- |
| macOS (Apple Silicon) | `.dmg` |
| Linux (x64) | `.deb` |

Il n’y a pas d’application Windows dans cette beta. Sous Windows 11, voir [Windows 11 (via WSL)](#windows-11-through-wsl).

<a id="download"></a>

## Télécharger
- [macOS (Apple Silicon) `.dmg`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-arm64.dmg)
- [macOS (Intel) `.dmg`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-x64.dmg)
- [Linux (x64) `.deb`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-amd64.deb)
- [Linux (ARM64) `.deb`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-arm64.deb)

Toutes les versions sont aussi sur [GitHub Releases](https://github.com/fertile-org/coppice/releases).

<a id="updates"></a>

## Mises à jour

Coppice cherche les nouvelles versions en arrière-plan. Pour désactiver ça, ouvrez **Settings → Updates** et désactivez **Automatic updates**. Vos tableaux et paramètres restent en place lors d’une mise à jour.

- **macOS :** choisissez **Download update** dans le bandeau, puis glissez le nouveau Coppice dans Applications.
- **Linux :** installer le `.deb` ajoute la source de paquets de Coppice, donc les nouvelles versions arrivent avec vos autres mises à jour dans le gestionnaire de mises à jour, ou lancez `sudo apt update && sudo apt upgrade`.

<a id="windows-11-through-wsl"></a>

## Windows 11 (via WSL)
La version Linux x64 devrait tourner sous Windows 11 via WSL. Nous ne l’avons pas encore testé. Si vous essayez, [racontez-nous](https://github.com/fertile-org/coppice/issues).

Il faut Windows 11, car les anciennes versions de Windows 10 ne peuvent pas afficher les fenêtres d’applications Linux. Les PC Windows sur Arm ne sont pas couverts pour l’instant.

1. Ouvrez PowerShell en administrateur et lancez `wsl --install`. Cela installe Ubuntu. Redémarrez quand c’est demandé, puis terminez la création de votre utilisateur Ubuntu.
2. Dans PowerShell en administrateur, mettez WSL à jour pour que les fenêtres d’applications Linux s’affichent sur votre bureau Windows :

   ```powershell
   wsl --update
   wsl --shutdown
   ```

3. Ouvrez Ubuntu depuis le menu Démarrer. Faites la suite des étapes dans Ubuntu. Téléchargez le `.deb` Linux x64 et installez-le. Remplacez `<version>` par la dernière version sur [GitHub Releases](https://github.com/fertile-org/coppice/releases) :

   ```sh
   cd ~
   wget https://github.com/fertile-org/coppice/releases/download/v<version>/Coppice-<version>-linux-amd64.deb
   sudo apt install ./Coppice-<version>-linux-amd64.deb
   ```

   Utilisez `apt`, pas `dpkg -i`. `apt` installe aussi ce dont Coppice a besoin.

   Pour mettre Coppice à jour plus tard, lancez `sudo apt update && sudo apt upgrade`. Vos tableaux et paramètres restent en place.
4. Installez dans Ubuntu les CLI d’agents que vous utilisez (Claude Code, Codex, Cursor, OpenCode, Kilo Code, GitHub Copilot, Qwen Code, Kimi Code). Connectez-vous à elles et à git là aussi.
5. Lancez Coppice depuis le menu Démarrer de Windows (sous Ubuntu), ou lancez `coppice` dans Ubuntu.

   Si une CLI affiche Not on your PATH, lancez Coppice avec `coppice` dans Ubuntu.

WSL ajoute votre PATH Windows à Ubuntu, donc Coppice pourrait trouver une copie Windows d’une CLI. Ouvrez **System → Connectors** et regardez le chemin de la CLI de chaque connecteur. Si un chemin commence par `/mnt/c/`, installez cette CLI dans Ubuntu.

Gardez vos dépôts dans votre dossier personnel Linux, par exemple `~/code/my-app`, pas sous `/mnt/c/`. Git et l’accès aux fichiers sont beaucoup plus lents à travers cette frontière. Le sélecteur de dossiers affiche des chemins Linux.

<a id="first-run"></a>

## Premier lancement
1. Installez et ouvrez Coppice.
2. Indiquez-lui un checkout git local que vous avez déjà sur le disque.
3. **Connectez votre CLI d’agent.** Choisissez votre CLI d’agent en créant un agent, et Coppice l’active. Chacune indique si elle est **Ready**, **Found, not signed in** ou **Not on your PATH**. Connectez-vous d’abord à la CLI dans votre terminal, comme d’habitude.

   Kilo Code, GitHub Copilot, Qwen Code et Kimi Code affichent **Installed** (par exemple **Kilo Code — Installed**). System → Connectors ajoute : « Coppice can't check this sign-in without using your plan. A run will show an error if you're signed out. » (Coppice ne peut pas vérifier cette connexion sans utiliser votre forfait. Un lancement affichera une erreur si vous êtes déconnecté.)

   Pour désactiver un connecteur plus tard, utilisez l’interrupteur dans System → Connectors, ou mettez `enabled = false` pour lui dans votre `config.toml` Coppice :

   - macOS: `~/Library/Application Support/Coppice/config.toml`
   - Linux: `~/.config/Coppice/config.toml`

4. Créez un ticket (ou approuvez-en un proposé) et passez-le en Ready. Les agents le font avancer dans In Progress, In Review et In QA, puis il attend votre Human Review.

<a id="requirements"></a>

## Prérequis
- Un dépôt git local dans lequel vous pouvez écrire (Coppice utilise des worktrees isolés)
- Au moins une CLI d’agent prise en charge, installée et authentifiée de la façon habituelle pour cet outil

<a id="troubleshooting"></a>

## Dépannage
Si l’application ne démarre pas ou qu’un agent ne prend jamais un ticket, regardez le terminal en direct sur le ticket et la FAQ. Pour compiler depuis les sources, voir la [documentation des contributeurs](https://github.com/fertile-org/coppice/blob/main/docs/development.md) dans le dépôt.
