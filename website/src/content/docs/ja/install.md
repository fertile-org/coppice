---
title: インストール
description: "Coppice Beta はデスクトップアプリとして配布しています。Docker もサーバーも Coppice アカウントも要りません。"
---

# インストール

Coppice Beta はデスクトップアプリとして配布しています。Docker もサーバーも Coppice アカウントも要りません。

<a id="supported-platforms"></a>

## 対応プラットフォーム
| プラットフォーム | パッケージ |
| --- | --- |
| macOS (Apple Silicon) | `.dmg` |
| Linux (x64) | `.deb` |

この Beta に Windows アプリはありません。Windows 11 の場合は [Windows 11（WSL 経由）](#windows-11-through-wsl)を見てください。

<a id="download"></a>

## ダウンロード
- [macOS (Apple Silicon) `.dmg`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-arm64.dmg)
- [macOS (Intel) `.dmg`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-mac-x64.dmg)
- [Linux (x64) `.deb`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-amd64.deb)
- [Linux (ARM64) `.deb`](https://github.com/fertile-org/coppice/releases/latest/download/Coppice-linux-arm64.deb)

すべてのビルドは [GitHub Releases](https://github.com/fertile-org/coppice/releases) からも入手できます。

<a id="updates"></a>

## アップデート

Coppice はバックグラウンドで新しいバージョンを確認します。オフにするには **Settings → Updates** を開き、**Automatic updates** をオフにします。アップデートしてもボードと設定はそのまま残ります。

- **macOS：** バナーの **Download update** を選び、新しい Coppice を Applications にドラッグします。
- **Linux：** `.deb` をインストールすると Coppice のパッケージソースが追加されるので、新しいバージョンはソフトウェアの更新でほかのアップデートと一緒に届きます。`sudo apt update && sudo apt upgrade` を実行しても構いません。

<a id="windows-11-through-wsl"></a>

## Windows 11（WSL 経由）
Linux x64 版は WSL を通して Windows 11 で動くはずです。まだ試していません。試した場合は、[結果を教えてください](https://github.com/fertile-org/coppice/issues)。

Windows 11 が必要です。古い Windows 10 のビルドでは Linux アプリのウィンドウを表示できません。Arm 版 Windows の PC は今のところ対象外です。

1. PowerShell を管理者として開き、`wsl --install` を実行します。Ubuntu がインストールされます。求められたら再起動し、Ubuntu のユーザー設定を済ませます。
2. 管理者の PowerShell で WSL を更新し、Linux アプリのウィンドウが Windows のデスクトップに出るようにします。

   ```powershell
   wsl --update
   wsl --shutdown
   ```

3. スタートメニューから Ubuntu を開きます。以降の手順は Ubuntu で行います。Linux x64 の `.deb` をダウンロードしてインストールします。`<version>` は [GitHub Releases](https://github.com/fertile-org/coppice/releases) にある最新のバージョンに置き換えてください。

   ```sh
   cd ~
   wget https://github.com/fertile-org/coppice/releases/download/v<version>/Coppice-<version>-linux-amd64.deb
   sudo apt install ./Coppice-<version>-linux-amd64.deb
   ```

   `dpkg -i` ではなく `apt` を使ってください。`apt` は Coppice に必要なものも一緒にインストールします。

   あとで Coppice をアップデートするには、`sudo apt update && sudo apt upgrade` を実行します。ボードと設定はそのまま残ります。
4. 使うエージェント CLI（Claude Code、Codex、Cursor、OpenCode、Kilo Code）を Ubuntu にインストールします。CLI と git へのサインインも Ubuntu で行います。
5. Windows のスタートメニュー（Ubuntu の下）から Coppice を起動するか、Ubuntu で `coppice` を実行します。

   CLI に Not on your PATH と表示されたら、Ubuntu で `coppice` を実行して Coppice を起動してください。

WSL は Windows の PATH を Ubuntu に追加するので、Coppice が Windows 側の CLI を見つけてしまうことがあります。**Tools → Connectors** を開き、各コネクタの CLI のパスを確認してください。パスが `/mnt/c/` で始まっていたら、その CLI を Ubuntu にインストールしてください。

リポジトリは `/mnt/c/` の下ではなく、Linux のホームフォルダー（たとえば `~/code/my-app`）に置いてください。その境界をまたぐと git やファイルへのアクセスがかなり遅くなります。フォルダーの選択画面には Linux のパスが表示されます。

<a id="first-run"></a>

## 初回の起動
1. Coppice をインストールして開きます。
2. ディスク上にすでにあるローカルの git チェックアウトを指定します。
3. **エージェント CLI をつなぐ。** エージェントを作るときにエージェント CLI を選ぶと、Coppice がそれをオンにします。それぞれ **Ready**、**Found, not signed in**、**Not on your PATH** のどれかが表示されます。まずはいつもどおりの方法で、ターミナルで CLI にサインインしてください。

   Kilo Code は **Kilo Code — Installed** と表示されます。Tools → Connectors には次の説明が付きます："Coppice can't check this sign-in without using your plan. A run will show an error if you're signed out."（Coppice はあなたのプランを使わずにこのサインインを確認できません。サインアウトしていると実行時にエラーが表示されます。）

   あとでコネクタをオフにするには、Tools → Connectors のトグルを使うか、Coppice の `config.toml` でそのコネクタに `enabled = false` を設定します。

   - macOS: `~/Library/Application Support/Coppice/config.toml`
   - Linux: `~/.config/Coppice/config.toml`

4. チケットを作成し（または提案されたチケットを承認し）、Ready に移します。エージェントが In Progress、In Review、In QA と進め、そのあとチケットはあなたの Human Review を待ちます。

<a id="requirements"></a>

## 必要なもの
- 書き込みできるローカルの git リポジトリ（Coppice は独立した worktree を使います）
- 対応するエージェント CLI を少なくとも 1 つ。そのツールのいつもの方法でインストールし、認証しておきます

<a id="troubleshooting"></a>

## トラブルシューティング
アプリが起動しない、またはエージェントがチケットに取りかからない場合は、チケットのライブターミナルとよくある質問を確認してください。ソースからビルドする方法は、リポジトリの[コントリビューター向けドキュメント](https://github.com/fertile-org/coppice/blob/main/docs/development.md)にあります。
