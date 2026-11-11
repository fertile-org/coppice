---
title: Claude Code と Codex を同じプロジェクトで使うには
metaTitle: 同じプロジェクトで Claude Code と Codex
description: "同じコードベースで Claude Code と Codex を動かす方法は 3 つです。ターミナルを 2 つ、git worktree、またはチケットごとにエージェントを置く 1 つのボードです。"
answer: "同時に同じファイルを編集しないよう、それぞれのツールにコードのコピーを持たせます。git worktree で手作業でもできますし、Coppice のようなボードでもできます。ボードではチケットごとにブランチがあり、Claude Code と Codex のどちらに任せるかを選べます。どちらでも、変更がメインブランチに入る前にあなたが確認します。"
published: "2026-11-07"
updated: "2026-11-08"
draft: false
author: Coppice
authorType: Organization
keywords:
  - Claude Code
  - Codex
  - コーディングエージェント
  - git worktree
---

## なぜ両方を使うのか

Claude Code と Codex は、どちらもターミナルで動くコーディングエージェントです。片方に計画をさせ、もう片方にコードを書かせることもできます。先に動いた方の変更を、もう一方のモデルにレビューさせることもできます。難しいのは起動することではありません。誰が何を変えたか分かるように、変更を分けておくことです。

よくあるやり方は 3 つです。

## 方法 1：ターミナル 2 つ、チェックアウトは 1 つ

プロジェクトを 2 つのターミナルで開きます。一方で `claude`、もう一方で `codex` を実行します。

始めるにはいちばん手早い方法です。一度に片方だけ使うとき、たとえば Claude Code が書いた内容を Codex にレビューさせるときに向いています。両方同時に編集すると、同じファイルを変え、後から書いた方が先の作業を上書きすることがあります。

## 方法 2：ツールごとに git worktree を 1 つ

git worktree は、同じリポジトリの 2 つ目の作業フォルダーで、専用のブランチを持ちます。Codex 用に作ります。

```bash
git worktree add ../my-project-codex -b codex-task
```

メインのフォルダーで `claude` を、`../my-project-codex` で `codex` を実行します。互いのファイルに触れずに、同時に作業できます。終わったら、ブランチごとに確認し、自分でマージします。

作業は分かれますが、調整は残ります。どの作業をどのツールに渡すか、間で文脈を渡すこと、使い終わったブランチの片付けは、あなたがします。

## 方法 3：Coppice の 1 つのボードに両方を置く

[Coppice](/) は、コーディングエージェントを作業用のボードに置くデスクトップアプリです。あなたのコンピューターにすでにある `claude` と `codex` のコマンドを実行します。チケットはそれぞれ自分のブランチで動くので、メインのチェックアウトには触れません。エージェントはボードを通してチケットを渡し合います。

1. [Coppice をインストール](/docs/install)します。ターミナルで、いつもの方法で Claude Code と Codex にサインインします。
2. プロジェクトのフォルダーを追加します。
3. エージェントを作り、それぞれ Claude Code か Codex を選びます。たとえば PM と Technical Lead は Claude Code、Backend Engineer は Codex、Reviewer は Claude Code にします。Codex が書いた内容を Claude Code がレビューします。
4. チケットを書き、エージェントを動かします。

PM が計画を書き、あなたが承認します。そのあと、エージェントが変更を作り、レビューします。最終の diff を読み、受け入れ、準備ができたらマージします。

![新しいエージェントの画面です。PM のプリセットが選ばれ、エージェントの CLI は Claude Code です。](/blog/claude-code-and-codex/new-agent.png "エージェントは 1 つの CLI で動きます。作るときに Claude Code か Codex を選びます。")

## どれを選ぶか

- **短い作業で、一度に 1 つのツール：** ターミナルを 2 つ。
- **並行して進め、ブランチの管理は自分でよい：** git worktree。
- **チケットが多く、エージェントが引き継ぎと互いのレビューをし、計画と diff はあなたが承認する：** Coppice。まずは[はじめに](/docs/getting-started)。
