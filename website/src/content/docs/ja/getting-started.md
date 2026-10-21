---
title: はじめに
description: "インストール直後から最初のエージェント実行まで、5 ステップで進みます。"
---

# はじめに

インストール直後から最初のエージェント実行まで、5 ステップで進みます。必要なのはローカルの git チェックアウトとエージェント CLI 1 つだけです。Coppice アカウントは要りません。

初めてインストールしたときは、最初のリポジトリ、ボード、スターターチームを追加する短いセットアップが開きます。あとから **System → Connectors** の **Set up workspace** で開き直せます。

<a id="1-install-a-provider-and-sign-in"></a>

## 1. プロバイダーをインストールしてサインインする
プロバイダーとは、Coppice があなたの代わりに動かすエージェント CLI のことです。Claude Code、Codex、Cursor、OpenCode、Kilo Code、GitHub Copilot、Qwen Code、Kimi Code、Grok があります。Coppice は CLI をインストールしません。コンピューターにすでにある CLI を見つけます。

1. いつもどおりの方法で CLI をインストールし、ターミナルでサインインします。
2. Coppice のサイドバーで **System** を開き、**Connectors** を開きます。
3. 使う CLI のカードを探します。Coppice が使える状態なら **Ready** と表示されます。

**Found, not signed in** と表示されたら、ターミナルで CLI にサインインしてください。**Not on your PATH** と表示されたら、CLI をインストールするか、カードの **Install docs** に従ってください。そのあと **Run check** をクリックします。

Kilo Code、GitHub Copilot、Qwen Code、Kimi Code は **Installed** と表示されます。Coppice はあなたのプランを使わずにこれらのサインインを確認できないので、サインアウトしていると実行時にエラーが表示されます。

<a id="2-set-up-an-agent"></a>

## 2. エージェントを用意する
1. サイドバーで **Agents** を開き、**New agent** をクリックします。
2. **Preset** を選びます。役割とプロンプトが入力されます。
3. **Name** を入力します。
4. **Connector** を選びます。Coppice は準備のできている最初のものを選択します。
5. **Model** を選ぶか、既定のままにします。
6. **Create agent** をクリックします。

保存すると、そのコネクタがオンになります。"{name} turned on." と表示されます。

**PM** エージェント、**Technical Lead** エージェント、そして作業をするエージェント（**Frontend Engineer**、**Backend Engineer**、**Research** のいずれか）を少なくとも 1 つ追加してください。計画と引き継ぎには 3 つすべてが必要です。

<figure>
<img src="/assets/docs/new-agent.webp" width="1440" height="900" alt="名前、役割、CLI を選んだ新しいエージェントのフォーム。">
<figcaption>各エージェントは、あなたの CLI のどれか 1 つで動きます。</figcaption>
</figure>

<a id="3-add-a-repository"></a>

## 3. リポジトリを追加する
1. サイドバーで **Repositories** を開き、**Add repository** をクリックします。
2. **Local path** の横の **Browse…** をクリックし、ローカルの git チェックアウトを選びます。
3. Coppice は可能な場合、チェックアウトから **Name**、**Remote URL (optional)**、**Default branch** を入力します。内容を確認してください。
4. **Add repository** をクリックします。

新しい行に **Ready** と表示されるはずです。**Path missing** や **Not a git repo** と表示されたら、フォルダーを選び直してください。

<a id="4-set-up-a-board"></a>

## 4. ボードを用意する
1. サイドバーで **Boards** を開きます。
2. **New board** をクリックします。初めてインストールしたときは **Create board** をクリックします。
3. **Name** を入力し、**Create board** をクリックします。

新しいボードが開きます。

<a id="5-create-a-ticket-and-run-it"></a>

## 5. チケットを作って実行する
1. **Backlog** 列の **Add ticket…** にタイトルを入力し、Enter を押します。
2. カードを開きます。
3. **Edit** をクリックして説明を書き、**Save** をクリックします。
4. **Detail** で **Assignee** にエージェントを、**Repository** にリポジトリを設定します。**Save metadata** をクリックします。
5. **Run agent** をクリックします。

チケットが **Live Console** に切り替わり、エージェントの作業を見られます。実行を止めるには **Stop** をクリックします。

チケットに担当者がいて、リポジトリが **Ready** になるまで、**Run agent** は押せません。エージェントを割り当てただけでは実行は始まりません。

<figure>
<img src="/assets/docs/backlog-ticket.webp" width="1440" height="900" alt="Backlog に新しいチケットがあり、Run agent ボタンが表示されているボード。">
<figcaption>Run agent を押すと、PM が計画を書きます。</figcaption>
</figure>

<a id="what-happens-next"></a>

## このあと起きること
1. **Plan Review**。PM エージェントが計画を書き、チケットが Plan Review に移ります。計画は説明のすぐ下に表示され、その下に **Approve plan** と **Ask for changes** があります。承認すると、計画はあなたの文章の後ろに **Approved plan** として説明に追加されます。
2. **Ready**。承認すると、チケットは Ready に移ります。Technical Lead エージェントがコードと照らして計画を確認し、必要なときだけ承認済みの計画を直して、担当者を選びます。その担当エージェントはすぐに動き始めます。適任者がいなければ担当者なしのままにして、理由をコメントします。
3. **In Progress**。割り当てられたエージェントが自分の git worktree で作業します。
4. **Wait for Human Review**。あなたが diff を読みます。Human Review で **Accept** をクリックします。
5. **Done**。Done のカードに "Accepted, not merged" と表示されることがあります。チケットを開いて **Merge…** を選ぶか、プルリクエストでレビューしたい場合は **Create PR via API** を選びます。

計画を飛ばすには、チケットの **Skip planning** をオンにします。チケットは Backlog から直接 Ready に進みます。
