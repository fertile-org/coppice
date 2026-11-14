import type { Pages } from './en';

export const locale = 'zh-tw';

/** 繁體中文（台灣） homepage and download copy. Keys mirror `./en.ts`; anything missing stays English. */
export const pages: Pages = {
  home: {
    title: 'Coppice：多代理程式設計的桌面應用程式',
    description:
      'Coppice 是多代理程式設計的桌面應用程式。你寫工單、批准計劃，並接受 diff。Claude Code、Codex 等代理在你的電腦上執行。',
    worksWith: '支援',
    h1: '僱用專家，你來拍板。',
    lede: '這是多代理程式設計的桌面應用程式，支援 Claude Code、Codex 等。你的 PM 代理起草計劃，由你批准。專業代理並行實作。你接受 diff 之前，都不算完成。',
    hero: {
      stillAlt: 'Coppice 看板，上面是 acme-web 倉庫的工單',
      videoAlt:
        'Coppice：寫一個工單，比較設計方案，批准 PM 代理起草的計劃，讓代理實作並測試，再對 diff 下評論。你接受之前，都不算完成。',
      captions: [
        '寫一個工單。',
        '你的 PM 代理起草計劃。',
        '它向你展示設計方案。',
        '你來批准。',
        '代理實作、審查、測試。',
        '審查 diff，可以評論任意一行。',
        '你接受之前，都不算完成。',
      ],
    },
    support: '開源（Apache 2.0）。在本機執行，不需要 Coppice 帳號。',
    downloads: {
      mac: '下載 macOS 版（Apple Silicon）',
      linux: '下載 Linux 版（.deb）',
      docs: '閱讀文件',
    },
    howTitle: '工作方式',
    steps: [
      {
        title: '把工作交出去。',
        body: '寫一個工單，或批准代理建議的工單。把它指派給 PM 代理。PM 代理會起草計劃，介面改動則會附上設計方案。寫程式開始前，你先批准計劃，除非你選擇 Skip planning（跳過規劃）。',
      },
      {
        title: '讓代理去實作。',
        body: '每個工單都有自己的 git worktree，並行的代理不會碰到你的主檢出目錄。每一次執行都可以即時查看。',
      },
      {
        title: '人工審查。',
        body: '在應用裡閱讀 diff，可以評論任意一行。你接受之前，都不算完成。',
      },
    ],
    howClose: '下載、開啟、開始做。不用另外設定：沒有 Docker，也沒有 Coppice 帳號。',
    whyTitle: '為什麼選擇 Coppice',
    reasons: [
      {
        title: '每個角色都用合適的模型。',
        body: '每個代理各自選擇 CLI 和模型。規劃和審查用你最強的模型，寫程式用較快的模型，都在同一塊看板上。',
      },
      {
        title: '不用再在終端之間來回切換。',
        body: '每個代理的工作都是同一塊看板上的一張卡片。受阻的工單，以及等你審查的工作，都會明顯標出來，你就知道該看哪裡。',
      },
      {
        title: '在合併之前找出錯誤。',
        body: '每個改動都會以 diff 的形式在應用裡等你審查。你接受之前，都不算完成；你接受的內容，就是最終合併的內容。',
      },
      {
        title: '不用再多一份訂閱。',
        body: 'Coppice 驅動的是你已經付費的 CLI：Claude Code、Codex、Cursor、OpenCode、Kilo Code、GitHub Copilot、Qwen Code、Kimi Code 和 Grok。',
      },
    ],
    notTitle: '它不是什麼',
    notItems: ['不是雲端程式設計服務', '不是 Jira 的替代品', '不是「告訴代理一句，就拿到成品」'],
  },
  download: {
    title: '下載 Coppice',
    metaTitle: '下載 Coppice（macOS 和 Linux）',
    description:
      '下載 Coppice：免費開源的 AI 程式設計代理看板，支援 macOS（Apple Silicon 或 Intel）和 Linux。在本機執行，不需要 Coppice 帳號。',
    intro: '免費，在你自己的電腦上執行，不需要 Coppice 帳號。',
    groups: [
      {
        note: '不確定這台 Mac 是哪一種？開啟蘋果選單 → 關於本機。如果顯示「晶片」，請選 Apple Silicon；如果顯示「處理器」，請選 Intel。',
        rows: [
          { name: 'macOS (Apple Silicon)', label: '下載 .dmg' },
          { name: 'macOS (Intel)', label: '下載 .dmg' },
        ],
      },
      {
        rows: [
          { name: 'Linux (x64)', label: '下載 .deb' },
          { name: 'Linux (ARM64)', label: '下載 .deb' },
        ],
      },
      {
        rows: [{ name: 'Windows 11', label: '透過 WSL 安裝' }],
      },
    ],
    checksum: {
      before: '可以用 ',
      sumsLabel: 'SHA256SUMS',
      between: ' 核對下載的檔案。作法見',
      installLabel: '安裝',
      after: '文件。',
    },
    next: {
      before: '接下來請依照',
      label: '安裝步驟',
      after: '操作。',
    },
  },
};
