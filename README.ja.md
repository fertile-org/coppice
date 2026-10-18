[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文（台灣）](README.zh-TW.md) · [繁體中文（香港）](README.zh-HK.md) · [日本語](README.ja.md) · [Français](README.fr.md)

# Coppice

**Beta 版。** 専門家を雇い、最終判断はあなたに。

<p align="center">
  <img src="static/marketing.gif" alt="チケットを書き、計画を承認し、diff をレビューする。あなたが承認するまで、何も完了にはならない。" width="960" />
</p>

**エージェントのターミナルを行き来するのはもう終わり。** チケットを書くと、PM エージェントが計画を下書きし、あなたの承認を待ちます。そのあと Claude Code、Codex などのエージェントが、それぞれ自分のブランチで並行して作り、レビューし、テストします。ボードを見れば、何が進んでいて、何が止まっていて、何があなたを待っているかが分かります。あなたが diff をレビューするまで、何も完了にはなりません。あなたのマシンで、すでに契約しているエージェント CLI を使って動きます。新しいサブスクリプションも Coppice アカウントも要りません。

## 2 つのゲート

作業は 2 か所であなたを待ちます。

1. **Plan Review**（計画レビュー）。PM エージェントが計画を下書きし、作業を始める前にあなたが承認するか修正を頼みます。計画づくりでチケットのコードが変わることはありません。Skip planning（計画をスキップ）をオンにすると、チケットは直接 Ready に進みます。
2. **Wait for Human Review**（人によるレビュー待ち）。あなたが diff をレビューします。承認するまで、チケットはこの列にとどまります。

作業を始めるのはあなたです。チケットを作るか、エージェントが提案したチケットを承認します。2 つのゲートの間で、エージェントが計画し、作ります。

## この Beta について

Beta のインストーラーは、macOS 用の `.dmg`（Apple Silicon または Intel）と Linux 用の `.deb`（`amd64` または `arm64`）です。[リリースページ](https://github.com/fertile-org/coppice/releases)から入手してください。

すでに使っている CLI をそのまま使えます：Claude Code、Codex、Cursor、OpenCode、Kilo Code。いつもの方法でインストールしてサインインしてください。Coppice はログインシェルの `PATH` からそれらを見つけます。

## はじめに

製品のドキュメントは [`website/`](website/) にあるサイトです：ランディングページとユーザーガイド。公開サイトは [https://getcoppice.app/](https://getcoppice.app/) です。

## コントリビューター向け

コントリビューターと CI は Docker Compose でスタックを動かします。まず [AGENTS.md](AGENTS.md) と [docs/development.md](docs/development.md) を読んでください。ロードマップは [docs/milestones/README.md](docs/milestones/README.md) にあります。

## ライセンス

Coppice は [Apache 2.0 ライセンス](LICENSE)のオープンソースです。
