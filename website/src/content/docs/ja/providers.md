---
title: プロバイダー
description: "Coppice が動かすエージェント CLI：Claude Code、Codex、Cursor、OpenCode、Kilo Code。"
---

# プロバイダー

エージェントごとにターミナルを開いておくのに疲れていませんか？ Coppice は、あなたがすでに使っているコーディングエージェントの CLI を動かし、その作業を 1 つのボードにまとめます。サブスクリプションとサインインはあなたのものをそのまま使います。Coppice アカウントは要りません。

<a id="supported-connectors"></a>

## 対応しているコネクタ

| コネクタ | Coppice が探すコマンド | インストール |
| --- | --- | --- |
| Claude Code | `claude` | [インストールガイド](https://docs.anthropic.com/en/docs/claude-code/setup) |
| Codex | `codex` | [インストールガイド](https://developers.openai.com/codex/cli) |
| Cursor | `agent` | [インストールガイド](https://cursor.com/docs/cli/installation) |
| OpenCode | `opencode` | [インストールガイド](https://opencode.ai/docs/) |
| Kilo Code | `kilo` | [インストールガイド](https://kilo.ai/docs/cli) |

<figure>
<img src="/assets/docs/model-picker.webp" width="1440" height="900" alt="エージェントのモデル選択。エージェントの CLI が提供するモデルが並んでいる。">
<figcaption>モデルを選ぶか、既定のままにします。</figcaption>
</figure>

このエージェントのモデルを選ぶか、既定のままにします。新しいモデルを入れたら、Refresh models をクリックしてください。

<a id="connect-your-agent-cli"></a>

## エージェント CLI をつなぐ

1. いつもどおりの方法で CLI をインストールし、ターミナルでサインインします。
2. エージェントを作るときにエージェント CLI を選ぶと、Coppice がそれをオンにします。

エージェントのフォームには、すべてのコネクタがステータス付きで並びます。

| ステータス | 意味 |
| --- | --- |
| **Ready** | 無料のサインイン確認で、サインイン済みであることを確かめました。 |
| **Found, not signed in** | コマンドは見つかりましたが、サインインしていません。ターミナルからサインインしてください。 |
| **Not on your PATH** | コマンドが見つかりません。インストールするか、PATH を確認してください。 |
| **Installed** | コマンドは見つかりました。Kilo Code の場合、プランを使わずにサインインを確認できません。 |

Kilo Code は **Kilo Code — Installed** と表示されます。Tools → Connectors には次の説明が付きます："Coppice can't check this sign-in without using your plan. A run will show an error if you're signed out." どの CLI でもチャットできます。Claude Code、Codex、Cursor、Kilo Code のチャットには "Coppice hasn't confirmed that {cli} stays read-only. The agent can still run commands on your computer." と表示されます。OpenCode のチャットにはバナーはありません。Test connection は `kilo` コマンドが動くかを確認します。

CLI の準備ができる前でもエージェントは保存できます。CLI をインストールしてサインインすれば、チケットの作業を始めます。Coppice を開いたあとで CLI をインストールした場合は、新しいコマンドを見つけられるよう Coppice を終了して開き直してください。

コネクタをオフにするには **Tools → Connectors** を使います。

<a id="use-your-own-api-key-or-another-provider"></a>

## 自分の API キーや別のプロバイダーを使う

自分の API キーや、CLI にないモデルを使いたい場合は OpenCode を使ってください。75 以上のモデルプロバイダーとローカルモデルに対応しています。[OpenCode のプロバイダー一覧](https://opencode.ai/docs/providers/)を見てください。

1. OpenCode をインストールします。
2. ターミナルで `opencode` を実行し、`/connect` と入力してプロバイダーを選び、キーを貼り付けます。キーは OpenCode が保管し、Coppice がキーを求めることはありません。一覧にない場合は、OpenCode の[カスタムプロバイダーの手順](https://opencode.ai/docs/providers/#custom-provider)に従ってください。
3. Coppice でエージェントを作り、**Connector** を **OpenCode** にします。プロバイダーのモデルが **Model** に表示されます。Coppice を開いたままプロバイダーを追加した場合は、**Refresh models** をクリックしてください。

<a id="on-a-ticket"></a>

## チケットでは

エージェントがチケットを引き受けると、Coppice はそのチケット専用の git worktree で CLI を起動します。進み具合はチケットの上でライブで見られます。

<a id="what-coppice-does-not-do"></a>

## Coppice がしないこと
- クラウドのコーディングサービスは動かしません
- あなたの Human Review なしにコードをマージしません
