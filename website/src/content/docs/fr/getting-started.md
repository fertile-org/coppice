---
title: Premiers pas
description: "D’une installation neuve à votre premier lancement d’agent en cinq étapes."
---

# Premiers pas

Cinq étapes vous mènent d’une installation neuve à votre premier lancement d’agent. Il vous faut un checkout git local et une CLI d’agent. Pas de compte Coppice.

À la première installation, Coppice ouvre une courte configuration qui ajoute votre premier dépôt, votre premier tableau et une équipe de départ. Vous pouvez la rouvrir depuis **System → Connectors** avec **Set up workspace**.

<a id="1-install-a-provider-and-sign-in"></a>

## 1. Installer un fournisseur et se connecter
Un fournisseur est la CLI d’agent que Coppice lance pour vous : Claude Code, Codex, Cursor, OpenCode, Kilo Code, GitHub Copilot, Qwen Code, Kimi Code ou Grok. Coppice ne l’installe pas. Il trouve les CLI déjà présentes sur votre ordinateur.

1. Installez la CLI et connectez-vous dans votre terminal, comme d’habitude.
2. Dans Coppice, ouvrez **System** dans la barre latérale, puis **Connectors**.
3. Trouvez la carte de votre CLI. Elle affiche **Ready** quand Coppice peut l’utiliser.

Si elle affiche **Found, not signed in**, connectez-vous à la CLI dans votre terminal. Si elle affiche **Not on your PATH**, installez-la, ou suivez **Install docs** sur la carte. Cliquez ensuite sur **Run check**.

Kilo Code, GitHub Copilot, Qwen Code, Kimi Code et Grok affichent **Installed**. Coppice ne peut pas vérifier leur connexion sans utiliser votre forfait, donc un lancement affiche une erreur si vous êtes déconnecté.

<a id="2-set-up-an-agent"></a>

## 2. Créer un agent
1. Ouvrez **Agents** dans la barre latérale et cliquez sur **New agent**.
2. Choisissez un **Preset**. Il remplit le rôle et le prompt.
3. Saisissez un **Name**.
4. Choisissez un **Connector**. Coppice sélectionne le premier qui est prêt.
5. Choisissez un **Model**, ou laissez celui par défaut.
6. Cliquez sur **Create agent**.

L’enregistrement active ce connecteur. Vous verrez « {name} turned on. ».

Ajoutez un agent **PM**, un agent **Technical Lead**, et au moins un agent qui fait le travail (**Frontend Engineer**, **Backend Engineer** ou **Research**). La planification et les passages de relais ont besoin des trois.

<figure>
<img src="/assets/docs/new-agent.webp" width="1440" height="900" alt="Le formulaire de nouvel agent avec un nom, un rôle et une CLI choisis.">
<figcaption>Chaque agent tourne sur l’une de vos CLI.</figcaption>
</figure>

<a id="3-add-a-repository"></a>

## 3. Ajouter un dépôt
1. Ouvrez **Repositories** dans la barre latérale et cliquez sur **Add repository**.
2. À côté de **Local path**, cliquez sur **Browse…** et choisissez un checkout git local.
3. Coppice remplit **Name**, **Remote URL (optional)** et **Default branch** à partir du checkout quand il le peut. Vérifiez-les.
4. Cliquez sur **Add repository**.

La nouvelle ligne doit afficher **Ready**. Si elle affiche **Path missing** ou **Not a git repo**, choisissez de nouveau le dossier.

<a id="4-set-up-a-board"></a>

## 4. Créer un tableau
1. Ouvrez **Boards** dans la barre latérale.
2. Cliquez sur **New board**. Sur une installation neuve, cliquez sur **Create board**.
3. Saisissez un **Name** et cliquez sur **Create board**.

Coppice ouvre votre nouveau tableau.

<a id="5-create-a-ticket-and-run-it"></a>

## 5. Créer un ticket et le lancer
1. Dans la colonne **Backlog**, saisissez un titre dans **Add ticket…** et appuyez sur Entrée.
2. Ouvrez la carte.
3. Cliquez sur **Edit** pour ajouter une description, puis sur **Save**.
4. Dans **Detail**, réglez **Assignee** sur votre agent et **Repository** sur votre dépôt. Cliquez sur **Save metadata**.
5. Cliquez sur **Run agent**.

Le ticket passe sur **Live Console** pour que vous puissiez suivre le travail de l’agent. Cliquez sur **Stop** pour arrêter le lancement.

**Run agent** reste désactivé tant que le ticket n’a pas de responsable et un dépôt qui affiche **Ready**. Assigner un agent ne lance rien à lui seul.

<figure>
<img src="/assets/docs/backlog-ticket.webp" width="1440" height="900" alt="Un tableau avec un nouveau ticket dans Backlog et le bouton Run agent visible.">
<figcaption>Appuyez sur Run agent, et le PM écrit le plan.</figcaption>
</figure>

<a id="what-happens-next"></a>

## Et ensuite
1. **Plan Review.** L’agent PM écrit un plan, et le ticket passe en Plan Review. Il attend là que vous approuviez le plan. Cliquez sur **Approve plan**.
2. **Ready.** Quand vous approuvez, le ticket passe en Ready. Les agents décident qui s’en charge ensuite. Si personne ne convient, ils le laissent sans responsable et ajoutent un commentaire qui explique pourquoi.
3. **In Progress.** L’agent assigné fait le travail dans son propre worktree git.
4. **Wait for Human Review.** Vous lisez le diff. Cliquez sur **Accept** dans Human Review.
5. **Done.** Une carte Done peut afficher « Accepted, not merged ». Ouvrez le ticket et choisissez **Merge…**, ou **Create PR via API** si vous préférez le relire sous forme de pull request.

Pour sauter le plan, cochez **Skip planning** sur le ticket. Il passe directement de Backlog à Ready.
