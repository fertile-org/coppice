---
title: プラグイン
metaTitle: "プラグイン：エージェントのスキルと MCP ツール"
description: "プラグインは Coppice のエージェントにスキルと MCP ツールを渡します。git やフォルダーから入れて有効にし、使えるエージェントを選ぶか、自作します。"
---

# プラグイン

プラグインはエージェントにスキルと MCP ツールを渡します。追加して有効にし、どのエージェントが使えるかを選びます。

プラグインはフォルダーです。スキル（`SKILL.md` ファイル）と MCP ツール（`.mcp.json`）を入れられます。管理するには、サイドバーで **Plugins** を開きます。

組み込みの `coppice` プラグインは常にあります。そのスキルはオフにできません。

<a id="install-a-plugin"></a>

## プラグインをインストールする
**Add plugins** を使います。方法は 2 つあります。

<a id="from-git"></a>

### git から
1. **Git URL** を貼り付けます。
2. 任意：`main` などの **Ref (optional)** を設定します。
3. **Target directory** を選びます。
4. **Install** をクリックします。

Coppice はあなたのコンピューターの git 認証情報でリポジトリをクローンします。あとで新しい変更を取り込むには、カードの **Update** をクリックします。

<a id="from-a-folder"></a>

### フォルダーから
1. **Plugin directories** の下で **Browse…** をクリックしてフォルダーを選びます。または **Directory path** に入力します。
2. **Add directory** をクリックします。

フォルダーは 1 つのプラグインでも、複数のプラグインをサブフォルダーとして持つものでも構いません。ファイルを変えたら **Rescan** をクリックします。

2 つのフォルダーに同じ名前のプラグインがある場合は、一覧で上にあるほうが使われます。矢印でフォルダーを並べ替えます。既定のフォルダーには **Default** と表示され、削除できません。

<a id="enable-it"></a>

## 有効にする
プラグインは無効の状態で始まります。

1. プラグインのカードのスイッチをオンにします。有効にできるのはステータスが `ok` のプラグインだけです。
2. カードに **Settings** がある場合は、各値を入力して **Save settings** をクリックします。値は暗号化して保存されます。
3. **Test** をクリックすると、プラグインのツールを起動して一覧にします。有効にする前でも使えます。

ローカルの MCP サーバーを持つプラグインは、あなたのコンピューター上で、あなたのコンピューターの権限で動きます。Coppice は先に確認を求めます。

スキルにはそれぞれスイッチがあります。オフにしたスキルは、実行中のものも含めてすべてのエージェントから見えなくなります。

<a id="choose-which-agents-can-use-it"></a>

## 使えるエージェントを選ぶ
カードの **Which agents can use it?** で、次のどちらかを選びます。

- **All agents**：あとで作るものも含めて、すべてのエージェント。
- **Choose agents**：チェックしたエージェントだけ。

エージェントのフォームの **Skills** でもプラグインを選べます。どちらで変えても同期されます。変更は各エージェントの次の実行から反映されます。

新しいプラグインの初期状態は次のとおりです。

- スキルだけ：**All agents**。
- MCP サーバーあり：**Choose agents**（誰もチェックされていない状態）。

<a id="use-it-on-a-ticket"></a>

## チケットで使う
プラグインを有効にし、チケットの担当者に使えるようにします。そのあと **Run agent** をクリックします。

エージェントには各スキルの名前と説明が見えていて、必要なときにスキルを読み込みます。ツールの名前は `<plugin>__<tool>` です。

何が使われたかを見るには、チケットの **Agent Runs** タブを開き、実行を展開して **Tools & Skills** を開きます。プラグインの呼び出しには **Plugin** の印が付きます。

<a id="create-a-plugin"></a>

## プラグインを作る
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

`plugin.json` がなくても、最上位に `SKILL.md` がある、または `skills/`、`.agents/skills/`、`.claude/skills/`、`<name>/SKILL.md` にスキルがあれば、そのフォルダーはプラグインになります。名前はフォルダー名、バージョンは `0.0.0` になります。

<a id="skills"></a>

### スキル
スキルはそれぞれ `SKILL.md` が入ったフォルダーです。フォルダー名がスキル名になります。ファイルはフロントマターで始めます。

```yaml
---
name: greeting-style
description: How to greet people in ticket comments. Load before writing a greeting.
---
```

`name` と `description` は必須です。エージェントがいつスキルを読み込めばよいか分かるように説明を書いてください。

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

`${NAME}` はプラグインのカードの設定項目になります。`${NAME:-default}` で既定値を付けられます。`${CLAUDE_PLUGIN_ROOT}` には Coppice がプラグインのインストール先フォルダーを入れます。これはカードの設定項目ではありません。SSE サーバーには対応していません。

この例では、ツール `hello-coppice__greet` とスキル `hello-coppice:greeting-style` が使えるようになります。完全な例は [`examples/plugins`](https://github.com/fertile-org/coppice/tree/main/examples/plugins) にあります。

`commands/`、`agents/`、`hooks/` フォルダーにはまだ対応していません。"Not supported yet" と表示され、実行されません。
