---
title: Fable で計画して、実装は安いモデルにするには
metaTitle: Fable で計画、実装は安いモデル
description: "計画は Fable 5.2、実装は Opus か Sonnet です。Claude Code には、計画だけ Fable にする設定がありません。Coppice ならエージェントごとにモデルを選べます。"
answer: "計画は Fable 5.2、実装は安い Opus か Sonnet にします。Claude Code にその設定はなく、opusplan は今も Opus のあと Sonnet です。/model fable で計画してから切り替えるか、subagent に別のモデルを持たせます。Coppice ではエージェントごとにモデルがあり、PM は Fable で計画し、エンジニアは Opus か Sonnet で実装できます。"
published: "2026-11-11"
draft: false
author: Coppice
authorType: Organization
keywords:
  - Fable
  - Opus
  - Sonnet
  - Claude Code
  - 計画
  - 実装
---

## なぜ難しいのか

Claude Fable 5.2 は、Anthropic が最も難しく、範囲の決まっていない作業向けに用意したモデルです。コーディングの多くは Opus 5.5 から始め、範囲の小さい作業には Sonnet 5.5 を使う、と Anthropic は言っています。Fable は Opus より高く、Opus は Sonnet より高いです。トークンのほとんどは計画のあとで、ファイルの読み取り、編集、再試行に使われます。そのあいだも Fable のままにすると、機械的な作業まで Fable がやります。あなたがほしいのは、計画は Fable、実装は Opus か Sonnet で、実装を始めるときに計画が残っていることです。

## 方法 1：Fable で計画してから切り替える

Claude Code には、計画を Fable、実装を安いモデルにする設定がありません。`opusplan` は今も、計画モードが Opus で、そのあとが Sonnet です。

`/model fable` を実行してから、計画モードに入ります。ターミナルでは、この別名は Fable 5.2 です。Claude apps gateway のセッションでは、同じ別名が今も Fable 5 なので、そこでは `/model claude-fable-5-2` を実行します。計画ができたら、`/model opus` か `/model sonnet` を実行します。会話はそのまま残ります。セッションが終わると、計画は消えます。

## 方法 2：subagent に自分のモデルを持たせる

Claude Code の subagent は Markdown ファイルです。`model` フィールドが、その subagent のモデルを決めます。値は `fable`、`opus`、`sonnet`、`haiku`、または完全な id です。セッションは Fable のままにし、実装は Opus か Sonnet に任せます。次のファイルを `.claude/agents/` に保存します。

```markdown
---
name: implementer
description: Implements an approved plan
model: opus
---
```

作業が小さいときは `sonnet` にします。そのモデルで動くのは、任せた作業だけです。メインのセッション自身が書くコードは、今も Fable で動きます。

## 方法 3：計画を安いセッションに貼る

Fable で計画し、その計画をコピーして、Opus か Sonnet でやり直します。

```bash
claude --model opus
```

作業が小さいときの同じ手順は、`claude --model sonnet` です。計画を貼り付けます。別のエージェント CLI にも、同じように貼れます。新しいセッションが知るのは、貼った内容だけです。

## 方法 4：Coppice で、エージェントごとにモデルを分ける

[Coppice](/) は、コーディングエージェントを作業用のボードに置くデスクトップアプリです。同じ CLI でも、エージェントごとにモデルを持てます。Settings のダークモード切り替えのようなチケットなら、PM は Fable 5.2 で計画し、Frontend Engineer は Opus 5.5 か Sonnet で実装できます。

1. [Coppice をインストール](/docs/install)します。ターミナルで、いつもの方法で Claude Code にサインインし、プロジェクトを追加します。
2. PM エージェントを作り、Claude Code を選び、モデルを Fable 5.2 にします。
3. Claude Code で Technical Lead と Frontend Engineer を作ります。Frontend Engineer のモデルは Opus 5.5 にします。変更が小さいときは Sonnet にします。
4. チケットを書き、エージェントを動かします。PM が Fable で計画を書きます。読んで、計画を承認するか、変更を求めます。計画の実行では、あなたのコードは変わりません。
5. コードの準備ができたら、diff を確認します。

あなたが承認したあと、Technical Lead が計画を確認し、Frontend Engineer を担当にします。Frontend Engineer の実行は、あなたが設定したモデルを使い、自動で始まります。[コンセプト](/docs/concepts)で、各手順を説明しています。

![PM という名前の、新しいエージェントの画面です。モデルの一覧が開いていて、Claude Fable 5.2 が選ばれています。](/blog/fable-for-planning/pm-fable.png "PM のモデル一覧で、Claude Fable 5.2 が選ばれています。")

![Frontend Engineer の、新しいエージェントの画面です。Claude Code が選ばれて Ready と出ていて、モデルは Claude Opus 5.5 です。](/blog/fable-for-planning/frontend-opus.png "Frontend Engineer は Claude Code を使い、モデルは Claude Opus 5.5 です。")

## どれを選ぶか

- **Claude Code のセッションが 1 つで、計画は Fable にしたい：** `/model fable` のあと、`/model opus` か `/model sonnet`。Opus と Sonnet で足りるときは `opusplan`。
- **任せた手順だけ安いモデルにしたい：** `model` を `opus` か `sonnet` にした subagent。
- **計画を新しいセッションに残したい：** Opus か Sonnet のセッションに貼ります。
- **チケットが多く、計画は Fable、実装は Opus か Sonnet：** Coppice。まずは[はじめに](/docs/getting-started)。
