---
title: プラグイン
metaTitle: "プラグイン：エージェントのスキルと MCP ツール"
description: "プラグインは Coppice のエージェントにスキルと MCP ツールを渡します。git やフォルダーから入れて有効にし、使えるエージェントを選ぶか、自作します。"
---

# プラグイン

プラグインはエージェントにスキルと MCP ツールを渡します。プラグインはフォルダーで、次のものを入れられます。

- **スキル**：`SKILL.md` ファイル。
- **MCP ツール**：`.mcp.json` ファイル。

管理するには、サイドバーで **Plugins** を開きます。組み込みの `coppice` プラグインは常にあり、そのスキルはオフにできません。

<a id="install-a-plugin"></a>

## 1. プラグインをインストールする
**Plugins** ページには 3 つのタブがあります。

- **Installed**：入手元を問わず、持っているすべてのプラグイン。
- **Marketplace**：最も手早く追加できる方法。
- **Custom**：git やフォルダーからインストール。

<a id="from-the-marketplace"></a>

### マーケットプレイスから
1. **Plugins** を開き、**Marketplace** タブに切り替えます。
2. 検索するか、ソースやカテゴリーで絞り込みます。
3. 項目の **Install** をクリックします。

<figure>
<img src="/assets/docs/plugin-marketplace.webp" width="1440" height="900" alt="Plugins ページの Marketplace タブ。検索、ソースのフィルター、Install ボタン付きのプラグインカードが表示されている。">
<figcaption>数千のプラグインから検索し、ワンクリックでインストールできます。</figcaption>
</figure>

プラグインは無効の状態で **Installed** タブに追加されます。次に[有効にします](#enable-it)。プラグインは、エージェントがどの CLI で動いていても、渡したすべてのエージェントで使えます。

<a id="marketplace-sources"></a>

#### ソース
最初から 5 つのソースが並んでいます。

- **coppice**：Coppice に付属するカタログ。
- **claude-plugins-official**：Anthropic の公式ディレクトリ。
- **claude-community**：Anthropic のコミュニティディレクトリ。
- **superpowers-marketplace**
- **ponytail**

ソースの読み込み：

- 各ソースは、**Marketplace** タブを初めて開いたときに自動で読み込まれます。
- 24 時間より古くなると、バックグラウンドで再度読み込まれます。
- 失敗したソースのチップにはエラーが表示されます。クリックすると、そのソースの読み込みを再試行します。

<a id="community-plugins"></a>

#### 誰がレビューしたか
- **Coppice** の印が付いたカードは Coppice のカタログのものです。
- それ以外のカードには **Community: not reviewed by Coppice** の印が付きます。有効にする前に内容を確認してください。
- `sha` を固定したカタログ項目は、ブランチが動いていても、そのコミットをインストールします。

<a id="manage-sources"></a>

#### ソースを管理する
**Manage sources** をクリックすると、各ソースと最後の更新日時を確認できます。

- **ソースを追加**：`.claude-plugin/marketplace.json` がある git リポジトリを追加します。Coppice はすぐにカタログを取得します。
- **再取得**：**Refresh** をクリックします。エラーの後は **Retry** です。
- **ソースを削除**：**coppice** 以外は削除できます。そのソースからインストールしたプラグインは残ります。削除した既定のソースは戻りません。

<a id="example-github"></a>

#### 例：GitHub
1. マーケットプレイスから `github` をインストールします。
2. 開いて、**Settings** の `GITHUB_PERSONAL_ACCESS_TOKEN` に GitHub の個人アクセストークンを貼り付け、**Save settings** をクリックします。
3. **Test** をクリックしてツールを一覧にします。
4. 有効にして、使えるエージェントを選びます。

このトークンは、Coppice がプルリクエストに使う **Settings** の GitHub トークンとは別のものです。

<a id="from-git"></a>

### git から
1. **Custom** タブを開きます。
2. **Git URL** を貼り付けます。
3. 任意：`main` などの **Ref (optional)** を設定します。
4. **Target directory** を選びます。
5. **Install** をクリックします。

Coppice はあなたのコンピューターの git 認証情報でリポジトリをクローンします。あとで新しい変更を取り込むには、詳細の **Update** をクリックします。

<a id="from-a-folder"></a>

### フォルダーから
1. **Custom** タブを開きます。
2. **Plugin directories** の下で **Browse…** をクリックしてフォルダーを選びます。または **Directory path** に入力します。
3. **Add directory** をクリックします。

- フォルダーは 1 つのプラグインでも、複数のプラグインをサブフォルダーとして持つものでも構いません。
- ファイルを変えたら **Rescan** をクリックします。
- 2 つのフォルダーに同じ名前のプラグインがある場合は、一覧で上にあるほうが使われます。矢印でフォルダーを並べ替えます。
- 既定のフォルダーには **Default** と表示され、削除できません。

<a id="enable-it"></a>

## 2. 有効にする
プラグインは無効の状態で始まります。

1. プラグインの行のスイッチをオンにします。有効にできるのはステータスが `ok` のプラグインだけです。
2. 詳細に **Settings** がある場合は、各値を入力して **Save settings** をクリックします。値は暗号化して保存されます。
3. **Test** をクリックすると、プラグインのツールを起動して一覧にします。有効にする前でも使えます。

- ローカルの MCP サーバーを持つプラグインは、あなたのコンピューター上で、あなたのコンピューターの権限で動きます。Coppice は先に確認を求めます。
- スキルにはそれぞれスイッチがあります。オフにしたスキルは、実行中のものも含めてすべてのエージェントから見えなくなります。

<a id="choose-which-agents-can-use-it"></a>

## 3. 使えるエージェントを選ぶ
詳細の **Which agents can use it?** で、次のどちらかを選びます。

- **All agents**：あとで作るものも含めて、すべてのエージェント。
- **Choose agents**：チェックしたエージェントだけ。

新しいプラグインの初期状態は次のとおりです。

- スキルだけ：**All agents**。
- MCP サーバーあり：**Choose agents**（誰もチェックされていない状態）。

- エージェントのフォームの **Skills** でもプラグインを選べます。どちらで変えても同期されます。
- 変更は各エージェントの次の実行から反映されます。

<a id="use-it-on-a-ticket"></a>

## 4. チケットで使う
1. プラグインを有効にします。
2. チケットの担当者に使えるようにします。
3. **Run agent** をクリックします。

- エージェントには各スキルの名前と説明が見えていて、必要なときにスキルを読み込みます。
- ツールの名前は `<plugin>__<tool>` です。

何が使われたかを見るには：

1. チケットの **Agent Runs** タブを開きます。
2. 実行を展開します。
3. **Tools & Skills** を開きます。プラグインの呼び出しには **Plugin** の印が付きます。

<a id="create-a-plugin"></a>

## 5. プラグインを作る
いちばん簡単なプラグインは、`SKILL.md` が入ったフォルダーです。名前、バージョン、MCP ツールを付けるなら、マニフェストを追加します。

```
hello-coppice/
├── .claude-plugin/
│   └── plugin.json
├── skills/
│   └── greeting-style/
│       └── SKILL.md
└── .mcp.json
```

### `plugin.json`
`.claude-plugin/plugin.json` に置きます。

```json
{
  "name": "hello-coppice",
  "version": "0.1.0",
  "description": "Example plugin: one skill and one MCP tool that greets people.",
  "author": { "name": "Coppice" }
}
```

| フィールド | 必須 | 説明 |
| --- | --- | --- |
| `name` | はい | 英字、数字、`.`、`_`、`-` で 64 文字まで。英字か数字で始めます。`coppice` は予約済みです。 |
| `version` | いいえ | 既定は `0.0.0`。 |
| `description` | いいえ | 既定は空。 |
| `author` | いいえ | 文字列、または `{ "name": "..." }`。 |
| `skills` | いいえ | スキルのディレクトリへのパス、またはディレクトリのパスのリスト。プラグインのルートからの相対パスです。 |

`plugin.json` がなくても、次のどちらかがあれば、そのフォルダーはプラグインになります。

- 最上位の `SKILL.md`。
- `skills/`、`.agents/skills/`、`.claude/skills/`、`<name>/SKILL.md` のスキル。

名前はフォルダー名、バージョンは `0.0.0` になります。

<a id="skills"></a>

### スキル
スキルはそれぞれ `SKILL.md` が入ったフォルダーです。フォルダー名がスキル名になります。ファイルはフロントマターで始めます。

```yaml
---
name: greeting-style
description: How to greet people in ticket comments. Load before writing a greeting.
---
```

- `name` と `description` は必須です。
- エージェントがいつスキルを読み込めばよいか分かるように説明を書いてください。

<a id="mcp-tools"></a>

### MCP ツール
`mcpServers` マップを持つ `.mcp.json` を追加します。サーバーはローカルかリモートのどちらかです。

- ローカル：`command`、任意で `args` と `env`。
- リモート：`"type": "http"`、`url`、任意で `headers`。

```json
{
  "mcpServers": {
    "hello": {
      "command": "node",
      "args": ["${CLAUDE_PLUGIN_ROOT}/server/index.mjs"],
      "env": { "HELLO_GREETING": "${HELLO_GREETING:-Hello}" }
    }
  }
}
```

- `${NAME}` はプラグイン詳細の設定項目になります。
- `${NAME:-default}` でその設定に既定値を付けられます。
- `${CLAUDE_PLUGIN_ROOT}` には Coppice がプラグインのインストール先フォルダーを入れます。これは設定項目ではありません。
- SSE サーバーには対応していません。

この例では、ツール `hello-coppice__greet` とスキル `hello-coppice:greeting-style` が使えるようになります。完全な例は [`examples/plugins`](https://github.com/fertile-org/coppice/tree/main/examples/plugins) にあります。

`commands/`、`agents/`、`hooks/` フォルダーにはまだ対応していません。"Not supported yet" と表示され、実行されません。
