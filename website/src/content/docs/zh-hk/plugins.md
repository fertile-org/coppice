---
title: 外掛
metaTitle: "外掛：代理技能與 MCP 工具"
description: "外掛為 Coppice 的代理提供技能和 MCP 工具。從 git 或資料夾安裝，啓用後選擇哪些代理可以使用，也可以自己撰寫外掛。"
---

# 外掛

外掛為代理提供技能和 MCP 工具。一個外掛就是一個資料夾，裏面可以包含：

- **技能**：`SKILL.md` 檔案。
- **MCP 工具**：一個 `.mcp.json` 檔案。

在側邊欄中開啓 **Plugins**（外掛）來管理它們。內建的 `coppice` 外掛始終存在，它的技能無法關閉。

<a id="install-a-plugin"></a>

## 1. 安裝外掛
**Plugins** 頁面有三個分頁：

- **Installed**：你擁有的所有外掛，無論來源。
- **Marketplace**：新增外掛最快的方式。
- **Custom**：從 git 或資料夾安裝。

<a id="from-the-marketplace"></a>

### 從外掛市集安裝
1. 開啓 **Plugins**，切換到 **Marketplace** 分頁。
2. 搜尋，或按來源、類別篩選。
3. 點選項目上的 **Install**。

<figure>
<img src="/assets/docs/plugin-marketplace.webp" width="1440" height="900" alt="Plugins 頁面的 Marketplace 分頁，包含搜尋框、來源篩選和帶 Install 按鈕的外掛卡片。">
<figcaption>在數千個外掛中搜尋，一鍵安裝。</figcaption>
</figure>

外掛會出現在 **Installed** 分頁，處於停用狀態。接下來請[啓用它](#enable-it)。無論代理執行在哪個 CLI 上，外掛都能供你授權的代理使用。

<a id="marketplace-sources"></a>

#### 來源
預設列出五個來源：

- **coppice**：Coppice 內建的目錄。
- **claude-plugins-official**：Anthropic 的官方目錄。
- **claude-community**：Anthropic 的社群目錄。
- **superpowers-marketplace**
- **ponytail**

來源的載入方式：

- 第一次開啓 **Marketplace** 分頁時，每個來源會自動載入。
- 如果來源已超過 24 小時，會在背景重新載入。
- 載入失敗的來源標籤會顯示錯誤，點選它可重試。

<a id="community-plugins"></a>

#### 誰審核過外掛
- 標記為 **Coppice** 的卡片來自 Coppice 目錄。
- 其他卡片都標記為 **Community: not reviewed by Coppice**。啓用前請先審核。
- 固定了 `sha` 的目錄項目會安裝那個確切的提交，即使分支已經變化。

<a id="manage-sources"></a>

#### 管理來源
點選 **Manage sources**，檢視每個來源及其上次重新整理時間。

- **新增來源**：新增一個包含 `.claude-plugin/marketplace.json` 的 git 倉庫。Coppice 會立即取得它的目錄。
- **重新取得**：點選 **Refresh**，出錯後點選 **Retry**。
- **移除來源**：除 **coppice** 外都可以移除。從該來源安裝的外掛會保留。移除的預設來源不會再回來。

<a id="example-github"></a>

#### 示例：GitHub
1. 從外掛市集安裝 `github`。
2. 開啓它，在 **Settings** 下把 GitHub 個人存取權杖貼到 `GITHUB_PERSONAL_ACCESS_TOKEN`，然後點選 **Save settings**。
3. 點選 **Test** 列出它的工具。
4. 啓用它，並選擇哪些代理可以使用。

這個權杖與 **Settings** 裏 Coppice 用於拉取請求的 GitHub 權杖是分開的。

<a id="from-git"></a>

### 從 git 安裝
1. 開啓 **Custom** 分頁。
2. 貼上 **Git URL**。
3. 可選：設定 **Ref (optional)**，例如 `main`。
4. 選擇 **Target directory**（目標目錄）。
5. 點選 **Install**。

Coppice 會用你電腦上的 git 憑據克隆倉庫。之後在詳情裏點選 **Update** 即可拉取新改動。

<a id="from-a-folder"></a>

### 從資料夾安裝
1. 開啓 **Custom** 分頁。
2. 在 **Plugin directories**（外掛目錄）下點選 **Browse…** 選擇資料夾，或直接在 **Directory path** 中輸入路徑。
3. 點選 **Add directory**。

- 這個資料夾可以本身就是一個外掛，也可以把多個外掛作為子資料夾放在裏面。
- 修改檔案後請點選 **Rescan**（重新掃描）。
- 如果兩個資料夾裏有同名外掛，列表中靠上的那個優先。用箭頭可以調整資料夾順序。
- 你的預設資料夾標有 **Default**，無法移除。

<a id="enable-it"></a>

## 2. 啓用外掛
外掛初始為停用狀態。

1. 開啓外掛列上的開關。只有狀態為 `ok` 的外掛才能啓用。
2. 如果詳情裏有 **Settings**，填寫每個值並點選 **Save settings**。這些值會加密儲存。
3. 點選 **Test**，啓動外掛的工具並列出它們。啓用之前也可以測試。

- 帶本地 MCP 伺服器的外掛會以你電腦的許可權在本機執行。Coppice 會先請你確認。
- 每個技能都有自己的開關。關閉的技能對所有代理都不可見，即使是已經在執行中的任務也一樣。

<a id="choose-which-agents-can-use-it"></a>

## 3. 選擇哪些代理可以使用
在詳情的 **Which agents can use it?**（哪些代理可以使用？）下選擇：

- **All agents**：所有代理，包括以後新建的。
- **Choose agents**：只有你勾選的代理。

新外掛的初始設定如下：

- 只包含技能：**All agents**。
- 包含 MCP 伺服器：**Choose agents**，且一個都沒勾選。

- 也可以在代理表單的 **Skills** 下選擇外掛。兩處設定保持同步。
- 改動從每個代理的下一次執行開始生效。

<a id="use-it-on-a-ticket"></a>

## 4. 在工單上使用
1. 啓用外掛。
2. 讓工單的負責人有權使用它。
3. 點選 **Run agent**。

- 代理能看到每個技能的名稱和描述，需要時再載入該技能。
- 工具的命名格式為 `<plugin>__<tool>`。

想檢視用了什麼：

1. 開啓工單的 **Agent Runs**（代理執行）標籤頁。
2. 展開一次執行。
3. 開啓 **Tools & Skills**。外掛呼叫會標記為 **Plugin**。

<a id="create-a-plugin"></a>

## 5. 建立外掛
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

沒有 `plugin.json` 時，只要符合以下任一條件，資料夾仍然算一個外掛：

- 頂層有 `SKILL.md`。
- 在 `skills/`、`.agents/skills/`、`.claude/skills/` 或 `<name>/SKILL.md` 中有技能。

外掛名取資料夾名，版本為 `0.0.0`。

<a id="skills"></a>

### 技能
每個技能是一個包含 `SKILL.md` 的資料夾，資料夾名就是技能名。檔案以 frontmatter 開頭：

```yaml
---
name: greeting-style
description: How to greet people in ticket comments. Load before writing a greeting.
---
```

- `name` 和 `description` 為必填項。
- description 要寫得讓代理知道什麼時候該載入這個技能。

<a id="mcp-tools"></a>

### MCP 工具
新增 `.mcp.json`，在其中寫一個 `mcpServers` 對映。每個伺服器可以是本地的，也可以是遠端的：

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

- `${NAME}` 會成為外掛詳情裏的一項設定。
- `${NAME:-default}` 為這項設定提供預設值。
- Coppice 會把 `${CLAUDE_PLUGIN_ROOT}` 替換為外掛的安裝資料夾，它不是一項設定。
- 不支援 SSE 伺服器。

這個示例提供了工具 `hello-coppice__greet` 和技能 `hello-coppice:greeting-style`。完整示例見 [`examples/plugins`](https://github.com/fertile-org/coppice/tree/main/examples/plugins)。

`commands/`、`agents/` 和 `hooks/` 資料夾暫不支援。它們會顯示 "Not supported yet"，並且不會執行。
