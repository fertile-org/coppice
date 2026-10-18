---
title: 設定リファレンス
description: "config.toml で変更できるすべての項目と、その型と既定値。"
---

# 設定リファレンス

Coppice は設定を 1 つのファイル `config.toml` に保存します。

| OS | パス |
| --- | --- |
| Linux | `~/.config/Coppice/config.toml` |
| macOS | `~/Library/Application Support/Coppice/config.toml` |

Linux で `XDG_CONFIG_HOME` が設定されている場合、ファイルは `$XDG_CONFIG_HOME/Coppice/config.toml` にあります。

<a id="edit-it-in-the-app"></a>

## アプリで編集する
サイドバーで **Settings** を開きます。ファイルがテキストで表示され、コメントもそのまま残ります。

- **Save** は書き込む前にファイルを確認します。不正な値は保存されません。
- **Restore last good version** は、まずい変更を元に戻します。

すぐに反映される変更もあれば、"Saved. Restart Coppice to apply."（保存しました。反映するには Coppice を再起動してください）と表示されるものもあります。

ファイルは任意のテキストエディターでも編集できます。編集内容はアップグレードしても残ります。

<a id="updates"></a>

## アップデート
**Settings → Updates** を開くと、バージョンを確認して **Check for updates** を選べます。**Automatic updates** は既定でオンです。オフにすると、Coppice はバックグラウンドで新しいバージョンを確認しなくなります。

このスイッチは `config.toml` にはありません。Coppice は同じフォルダーの `update-settings.json` に `automaticUpdates` として保存します。

<a id="replace-keys"></a>

## 鍵を入れ替える

バックアップを共有した場合は、新しい鍵と新しい管理者パスワードを作ってください。保存済みのトークンはそのまま使え、全員がサインアウトされます。

**Tools → Backup → Replace keys** を開き、新しい管理者パスワードを入力して確定します。

<a id="logs"></a>

## ログ
管理者は Settings の横の **Logs** を開き、サーバー、リクエスト、エージェントの実行、データベースのログを絞り込んだり、ライブで追いかけたりできます。各実行には **View logs** のリンクがあります。

**Export logs** は、いまの絞り込みに合う行を `coppice-logs.zip` として保存します。Coppice は把握しているトークンと鍵を取り除きますが、共有する前にファイルを確認してください。

**Report a problem** は、Coppice のバージョンを記入済みの新しい GitHub issue を開きます。何が起きて、何を期待していたかを書き、**Export logs** で作った zip を添付してください。

## `[logs]`

| フィールド | 型 | 既定値 | 内容 |
| --- | --- | --- | --- |
| `retention_days` | integer | `14` | 今日を含めて何日分のログを残すか：3、7、14、30。既定は 14 です。Coppice は 1 時間ごとに古いログを削除し、変更は再起動なしで反映されます。ログが 256 MiB を超えると、`retention_days` に達する前でも古いものから削除します。 |

1 日のログは最大 32 MiB のファイルに分かれ、今日の分も含めてすべてのファイルが 256 MiB の上限に数えられます。Coppice は、書き込み中のログファイルとデータベースのログは削除しません。データベースのログも上限に数えられ、`retention_days` を過ぎると削除されます。

| フィールド | 型 | 既定値 | 内容 |
| --- | --- | --- | --- |
| `dir` | string | `./data/logs` | ログのフォルダー。デスクトップアプリは独自のフォルダーを使い、この値を無視します。変更には再起動が必要です。 |

<a id="connectors"></a>

## コネクタ
サインイン済みの CLI は既定でオンです。オフにしたいときだけ 1 行書きます。

```toml
[agent.connectors.claude-code]
enabled = false
```

**Tools → Connectors** のスイッチも同じ行を書き込みます。

Kilo Code は、あなたがオンにするまでオフのままです。Coppice は無料でそのサインインを確認できないためです。

## `[agent]`

| フィールド | 型 | 既定値 | 内容 |
| --- | --- | --- | --- |
| `worktrees_path` | string | `<data-dir>/worktrees` | チケットごとの git worktree を置くフォルダー。 |
| `worker_count` | integer | `2` | 同時に動くエージェントのジョブの数。1 未満は 1 として扱います。 |
| `health_check_interval_secs` | integer | `60` | エージェントのヘルスチェックの間隔（秒）。10 未満は 10 として扱います。 |

`<data-dir>` は `config.toml` があるフォルダーです。Linux と WSL では `~/.config/Coppice`、macOS では `~/Library/Application Support/Coppice` です。

## `[agent.connectors.<id>]`

ID：`claude-code`、`codex`、`cursor`、`opencode`、`kilo-code`。どのテーブルも省略できます。

| フィールド | 型 | 既定値 | 内容 |
| --- | --- | --- | --- |
| `enabled` | bool | 未設定：CLI がサインイン済みならオン | コネクタをオンまたはオフにします。 |
| `model_providers` | list of strings | `[]` | このコネクタのモデルプロバイダー。オンにしたときに入力されます。 |
| `run_timeout_secs` | integer | OpenCode は `1800`、ほかは `600` | このコネクタでの実行が失敗とみなされるまでの秒数。 |
| `command` | string | `agent`（Cursor）、`codex`、`opencode`、`kilo` | 実行する CLI のバイナリ。Claude Code は常に `claude` を実行し、`command` はありません。 |

OpenCode のみ：

| フィールド | 型 | 既定値 | 内容 |
| --- | --- | --- | --- |
| `serve_hostname` | string | `127.0.0.1` | 実行中に使う OpenCode のヘルパーのホスト名。再起動が必要です。 |

OpenCode の `command` を変えた場合も再起動が必要です。

## `[workflow]`

| フィールド | 型 | 既定値 | 内容 |
| --- | --- | --- | --- |
| `auto_start_runs` | bool | `false` | true の場合、リポジトリのあるチケットにエージェントを割り当てると実行が始まります。false の場合は **Run agent** をクリックします。 |
| `auto_assign.default` | bool | `true` | 個別の設定がないすべてのステータスで自動割り当てをオンにします。 |
| `auto_assign.<status>` | bool | `backlog`：`false`。ほか：`default` に従う | 1 つのステータスについて自動割り当てをオンまたはオフにします。 |
| `auto_split.default` | bool | `false` | 個別の設定がないすべてのステータスで自動分割をオンにします。 |
| `auto_split.<status>` | bool | `default` に従う | 1 つのステータスについて自動分割をオンまたはオフにします。 |

ステータスのキー：`backlog`、`ready`、`in_progress`、`in_review`、`in_qa`、`wait_for_final_review`（Wait for Human Review）、`blocked`、`done`。

そのステータスで自動割り当てがオンなら、Coppice は実行が指名したエージェントを割り当てます。オフなら、チケットには代わりに **Recommends:** が表示されます。

そのステータスで自動分割がオンなら、Coppice は実行が提案した子チケットを作ります。オフなら、**Approve splits** をクリックするまで **Pending split recommendation** として保存します。

## `[knowledge]`

| フィールド | 型 | 既定値 | 内容 |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | Knowledge の圧縮をオンまたはオフにします。 |
| `poll_interval_ms` | integer | `500` | 圧縮スケジューラーが起きる間隔。100 未満は 100 として扱います。 |
| `compaction.interval_secs` | integer | `1800` | 圧縮サイクルの間隔（秒）。 |
| `compaction.batch_max_tickets` | integer | `10` | 1 回のバッチに入るチケットの最大数。 |
| `compaction.batch_max_source_bytes` | integer | `200000` | 1 回のバッチに入るソースの最大バイト数。 |
| `compaction.max_candidates_per_batch` | integer | `20` | 1 回のバッチが提案できる候補の最大数。 |
| `compaction.max_attempts` | integer | `3` | 1 回のバッチを試す最大回数。 |
| `auto_save.enabled` | bool | `false` | リスクの低い候補を確認なしで保存します。 |
| `auto_save.allowed_types` | list of strings | `[]` | 自動保存してよい種類：`bug_pattern`、`coding_convention`、`dependency_note`、`performance_note`、`review_feedback`、`test_command`。 |
| `auto_save.minimum_confidence` | string | `high` | `high` のままにする必要があります。 |
| `retrieval.top_k` | integer | `8` | 1 回の検索で返す件数。1〜20。 |
| `retrieval.allowed_types` | list of strings | `[]`（すべての種類） | これらの種類だけを返します：`coding_convention`、`architecture_rule`、`bug_pattern`、`test_command`、`review_feedback`、`dependency_note`、`api_contract`、`workflow_rule`、`human_preference`、`operational_runbook`、`security_rule`、`performance_note`。 |
| `retrieval.minimum_confidence` | string | `medium` | `low`、`medium`、`high` のいずれか。 |
| `retrieval.default_page_size` | integer | `25` | 既定のページサイズ。 |
| `retrieval.max_page_size` | integer | `100` | 最大のページサイズ。最大 100 です。 |
| `retrieval.max_active_per_board` | integer | `1000` | 1 つのボードが持てる Knowledge 項目（保留中または承認済み）の最大数。そのボードのエージェントの項目も数えます。 |
| `retrieval.max_active_workspace` | integer | `10000` | ワークスペース全体に適用される Knowledge 項目（保留中または承認済み）の最大数。 |

上の件数と間隔はすべて 0 より大きくする必要があります。

## `[git]`

| フィールド | 型 | 既定値 | 内容 |
| --- | --- | --- | --- |
| `author_name` | string | `Coppice` | Coppice が作るコミットの作者名。 |
| `author_email` | string | `coppice@localhost` | それらのコミットの作者のメールアドレス。 |
| `push_enabled` | bool | `false` | Coppice にブランチのプッシュとプルリクエストの作成を許可します。 |

## `[plugins]`

| フィールド | 型 | 既定値 | 内容 |
| --- | --- | --- | --- |
| `dir` | string | `<data-dir>/plugins` | 既定のプラグインフォルダー。Plugins ページでフォルダーを追加できます。 |
| `allow_file_git_urls` | bool | `false` | プラグインのインストールに `file:` の git URL を使えるようにします。 |
| `git_timeout_secs` | integer | `300` | プラグインのクローンや更新が失敗とみなされるまでの秒数。 |
| `mcp_start_timeout_secs` | integer | `20` | プラグインの MCP サーバーの起動を待つ秒数。 |
| `mcp_list_timeout_secs` | integer | `10` | プラグインがツールを一覧にするのを待つ秒数。 |
| `mcp_idle_shutdown_secs` | integer | `600` | 使われていないプラグインの MCP サーバーを動かしておく秒数。 |

## `[mcp]`

| フィールド | 型 | 既定値 | 内容 |
| --- | --- | --- | --- |
| `builtin_plugins_dir` | string | `<data-dir>/builtin-plugins` | Coppice が起動のたびに組み込みスキルを書き出す場所。ここでの編集は失われます。 |
| `call_timeout_secs` | integer | `60` | 1 回のツール呼び出しが失敗とみなされるまでの秒数。 |
| `max_output_bytes` | integer | `32768` | ツールの結果の最大バイト数。 |
| `comment_post_limit` | integer | `5` | 1 回の実行が投稿できるコメントの最大数。 |
| `token_ttl_secs` | integer | `14400` | 実行のツールトークンが有効な秒数。 |

## `[keybindings]`

各キーは[キーボードショートカット](/docs/shortcuts)のコマンド ID です。あなたのキーがそのコマンドの既定のキーに置き換わり、空の値にするとショートカットがオフになります。

| フィールド | 型 | 既定値 | 内容 |
| --- | --- | --- | --- |
| `"<command id>"` | string or list of strings | 組み込みのショートカット | そのコマンドのキーを置き換えます。`""` で削除します。 |

```toml
[keybindings]
"palette.open" = "P"
"ticket.create" = ""
"board.nextCard" = ["J", "Down"]
```

`Mod` は macOS では Cmd、Linux と WSL では Ctrl です。連続したキーは `G then B` のように書きます。1 つのコマンドに複数のキーを割り当てるときは ` or ` で区切るか、リストで書きます。Save すると再起動なしで反映されます。Coppice が読めないショートカット、存在しないコマンド ID、ほかのコマンドがすでに使っているキー、ブラウザーやシステムのキーは使われません。そのコマンドは既定のキーのままになり、Settings に理由が表示されます。
