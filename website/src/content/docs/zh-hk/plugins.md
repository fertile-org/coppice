---
title: 外掛
metaTitle: "外掛：代理技能與 MCP 工具"
description: "外掛為 Coppice 的代理提供技能和 MCP 工具。從 git 或資料夾安裝，啓用後選擇哪些代理可以使用，也可以自己撰寫外掛。"
---

# 外掛

外掛為代理提供技能和 MCP 工具。新增外掛、啓用它，再選擇哪些代理可以使用。

一個外掛就是一個資料夾，裏面可以包含技能（`SKILL.md` 檔案）和 MCP 工具（`.mcp.json`）。在側邊欄中開啓 **Plugins**（外掛）來管理它們。

內建的 `coppice` 外掛始終存在，它的技能無法關閉。

<a id="install-a-plugin"></a>

## 安裝外掛
使用 **Add plugins**（新增外掛）。有兩種方式。

<a id="from-git"></a>

### 從 git 安裝
1. 貼上 **Git URL**。
2. 可選：設定 **Ref (optional)**，例如 `main`。
3. 選擇 **Target directory**（目標目錄）。
4. 點選 **Install**。

Coppice 會用你電腦上的 git 憑據克隆倉庫。之後在卡片上點選 **Update** 即可拉取新改動。

<a id="from-a-folder"></a>

### 從資料夾新增
1. 在 **Plugin directories**（外掛目錄）下點選 **Browse…** 選擇資料夾，或直接在 **Directory path** 中輸入路徑。
2. 點選 **Add directory**。

這個資料夾可以本身就是一個外掛，也可以把多個外掛作為子資料夾放在裏面。修改檔案後請點選 **Rescan**（重新掃描）。

如果兩個資料夾裏有同名外掛，列表中靠上的那個優先。用箭頭可以調整資料夾順序。你的預設資料夾標有 **Default**，無法移除。

<a id="enable-it"></a>

## 啓用外掛
外掛初始為停用狀態。

1. 開啓外掛卡片上的開關。只有狀態為 `ok` 的外掛才能啓用。
2. 如果卡片上有 **Settings**，填寫每個值並點選 **Save settings**。這些值會加密儲存。
3. 點選 **Test**，啓動外掛的工具並列出它們。啓用之前也可以測試。

帶本地 MCP 伺服器的外掛會以你電腦的許可權在本機執行。Coppice 會先請你確認。

每個技能都有自己的開關。關閉的技能對所有代理都不可見，即使是已經在執行中的任務也一樣。

<a id="choose-which-agents-can-use-it"></a>

## 選擇哪些代理可以使用
在卡片的 **Which agents can use it?**（哪些代理可以使用？）下選擇：

- **All agents**：所有代理，包括以後新建的。
- **Choose agents**：只有你勾選的代理。

也可以在代理表單的 **Skills** 下選擇外掛。兩處設定保持同步，改動從每個代理的下一次執行開始生效。

新外掛的初始設定如下：

- 只包含技能：**All agents**。
- 包含 MCP 伺服器：**Choose agents**，且一個都沒勾選。

<a id="use-it-on-a-ticket"></a>

## 在工單上使用
啓用外掛，並讓工單的負責人有權使用它，然後點選 **Run agent**。

代理能看到每個技能的名稱和描述，需要時再載入該技能。工具的命名格式為 `<plugin>__<tool>`。

想檢視用了什麼，開啓工單的 **Agent Runs**（代理執行）標籤頁，展開一次執行，再開啓 **Tools & Skills**。外掛呼叫會標記為 **Plugin**。

<a id="create-a-plugin"></a>

## 建立外掛
最簡單的外掛就是一個包含 `SKILL.md` 的資料夾。如果需要名稱、版本和 MCP 工具，請新增清單檔案。

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
放在 `.claude-plugin/plugin.json`。

```json
{
  "name": "hello-coppice",
  "version": "0.1.0",
  "description": "Example plugin: one skill and one MCP tool that greets people.",
  "author": { "name": "Coppice" }
}
```

| 欄位 | 必填 | 説明 |
| --- | --- | --- |
| `name` | 是 | 字母、數字、`.`、`_` 和 `-`，最多 64 個字元，必須以字母或數字開頭。`coppice` 為保留名稱。 |
| `version` | 否 | 預設為 `0.0.0`。 |
| `description` | 否 | 預設為空。 |
| `author` | 否 | 字串，或 `{ "name": "..." }`。 |
| `skills` | 否 | 一個技能目錄的路徑，或目錄路徑列表，相對於外掛根目錄。 |

沒有 `plugin.json` 時，只要資料夾頂層有 `SKILL.md`，或者在 `skills/`、`.agents/skills/`、`.claude/skills/` 或 `<name>/SKILL.md` 中有技能，它仍然算一個外掛。外掛名取資料夾名，版本為 `0.0.0`。

<a id="skills"></a>

### 技能
每個技能是一個包含 `SKILL.md` 的資料夾，資料夾名就是技能名。檔案以 frontmatter 開頭：

```yaml
---
name: greeting-style
description: How to greet people in ticket comments. Load before writing a greeting.
---
```

`name` 和 `description` 為必填項。description 要寫得讓代理知道什麼時候該載入這個技能。

<a id="mcp-tools"></a>

### MCP 工具
新增 `.mcp.json`，在其中寫一個 `mcpServers` 對映。每個伺服器可以是本地的，也可以是遠端的。

- 本地：`command`，以及可選的 `args` 和 `env`。
- 遠端：`"type": "http"`、`url`，以及可選的 `headers`。

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

`${NAME}` 會成為外掛卡片上的一項設定，`${NAME:-default}` 則為它提供預設值。Coppice 會把 `${CLAUDE_PLUGIN_ROOT}` 替換為外掛的安裝資料夾，它不是卡片上的設定。不支援 SSE 伺服器。

這個示例提供了工具 `hello-coppice__greet` 和技能 `hello-coppice:greeting-style`。完整示例見 [`examples/plugins`](https://github.com/fertile-org/coppice/tree/main/examples/plugins)。

`commands/`、`agents/` 和 `hooks/` 資料夾暫不支援。它們會顯示 "Not supported yet"，並且不會執行。
