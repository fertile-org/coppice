---
title: よくある質問
metaTitle: "Coppice のよくある質問：Beta、Docker、Windows"
description: "Coppice は本番で使える？Docker やアカウントは必要？エージェントが勝手にマージする？Beta、Windows、コンフリクトなどへの回答。"
---

# よくある質問

<a id="is-coppice-production-ready"></a>

## Coppice は本番で使えますか？
いいえ、まだです。Coppice は Beta 版で、本物の人間のゲートを備えたローカルのエージェントボードを試したい早期ユーザー向けです。エージェントはあなたのローカル権限で動きます。

<a id="do-i-need-docker-or-an-account"></a>

## Docker やアカウントは必要ですか？
いいえ。デスクトップアプリをダウンロードして開けば、すぐに使えます。Docker もサーバーの設定も Coppice アカウントも要りません。

<a id="is-this-a-jira--linear-replacement"></a>

## Jira や Linear の代わりになりますか？
いいえ。Coppice はコーディングエージェントを動かすためのボードで、会社全体の課題管理ツールではありません。

<a id="is-this-a-cloud-coding-agent"></a>

## クラウドのコーディングエージェントですか？
いいえ。エージェントはあなたのマシン上の CLI を通して、あなたのリポジトリで動きます。違いを言うと、クラウドのエージェントサービスは作業を他人のコンピューターに任せます。Coppice は作業をローカルに置き、あなたのゲートを通します。

<a id="will-agents-merge-without-me"></a>

## エージェントが勝手にマージしますか？
いいえ。Human Review は正式なゲートです。diff を開いて決めるのはあなたです。

<a id="what-happens-if-a-tickets-changes-conflict-with-main"></a>

## チケットの変更が main と競合したらどうなりますか？
Coppice はマージの前で止まり、main には手を付けません。競合するファイルを一覧にして、チケットを In Progress に戻し、同じエージェントが解決に取りかかります。その後、マージの前に Human Review でもう一度承認します。

<a id="what-about-set-and-forget-autopilots"></a>

## 「任せっぱなし」の自動運転はどうですか？
それは Coppice の目指すものではありません。Coppice はエージェントを管理するためのものです。作業を始めるのはあなたで、すべての変更を承認するのもあなたです。

<a id="windows"></a>

## Windows は？
Windows 11 では WSL を通して Linux x64 版が動くはずですが、まだ試していません。[Windows 11（WSL 経由）](/docs/install#windows-11-through-wsl)を見てください。試した場合は、[GitHub で結果を教えてください](https://github.com/fertile-org/coppice/issues)。この Beta でネイティブの Windows アプリを出す予定はありません。

<a id="where-do-contributors-look"></a>

## 貢献したい人はどこを見ればいいですか？
アーキテクチャ、開発、テストのドキュメントは [GitHub リポジトリ](https://github.com/fertile-org/coppice/tree/main/docs)にあります。このサイトは Coppice を使うためのものです。
