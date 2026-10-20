import type { Pages } from './en';

export const locale = 'zh-hk';

/** 繁體中文（香港） homepage and download copy. Keys mirror `./en.ts`; anything missing stays English. */
export const pages: Pages = {
  home: {
    description:
      '各有分工的程式設計代理在同一塊看板上規劃、構建和審查，你接受 diff 之前，什麼都不算完成。本地執行，開源，用的是你已經付費的代理 CLI。',
    h1: '僱用專家，你來拍板。',
    lede: '你的 PM 代理起草計劃，由你批准。專業代理並行構建。你接受 diff 之前，什麼都不算完成。',
    hero: {
      stillAlt: 'Coppice 看板，上面是 acme-web 倉庫的工單',
      videoAlt:
        'Coppice：寫一個工單，批准 PM 代理起草的計劃，讓代理構建和測試，然後對 diff 發表評論。你接受之前，什麼都不算完成。',
      captions: [
        '寫一個工單。',
        '你的 PM 代理起草計劃。',
        '你來批准。',
        '代理構建、審查、測試。',
        '審查 diff，可以評論任意一行。',
        '你接受之前，什麼都不算完成。',
      ],
    },
    support: '開源（Apache 2.0）。本地執行，無需 Coppice 帳號。',
    downloads: {
      mac: '下載 macOS 版（Apple Silicon）',
      linux: '下載 Linux 版（.deb）',
      docs: '閲讀文件',
    },
    howTitle: '工作方式',
    steps: [
      {
        title: '把工作交出去。',
        body: '寫一個工單，或批准代理建議的工單。把它指派給你的 PM 代理，它會起草一份計劃，經你批准後才會有人寫程式碼。',
      },
      {
        title: '讓代理去構建。',
        body: '每個工單都有自己的 git worktree，並行的代理永遠不會碰你的主檢出目錄。任何一次執行都可以即時檢視。',
      },
      {
        title: '人工審查。',
        body: '在應用裏閲讀 diff，可以評論任意一行，你接受之前，什麼都不算完成。',
      },
    ],
    howClose: '下載、開啓、開幹。無需任何配置：不用 Docker，也不用 Coppice 帳號。',
    whyTitle: '為什麼選擇 Coppice',
    reasons: [
      {
        title: '每個角色都用合適的模型。',
        body: '每個代理各自選擇 CLI 和模型。用最強的模型做規劃和審查，用更快的模型寫程式碼，全都在同一塊看板上。',
      },
      {
        title: '不用再在終端之間來回切換。',
        body: '每個代理的工作都是同一塊看板上的一張卡片。受阻的工單和等你審查的工作一目瞭然，你知道哪裏需要你。',
      },
      {
        title: '在落地之前發現錯誤。',
        body: '每個改動都會以 diff 的形式在應用裏等你審查。你接受之前，什麼都不算完成；你接受的內容，就是最終合併的內容。',
      },
      {
        title: '不用再多一份訂閲。',
        body: 'Coppice 驅動的是你已經付費的 CLI：Claude Code、Codex、Cursor、OpenCode、Kilo Code、GitHub Copilot、Qwen Code、Kimi Code 和 Grok。',
      },
    ],
    notTitle: '它不是什麼',
    notItems: ['不是雲端程式設計服務', '不是 Jira 的替代品', '不是“告訴代理一句，就拿到成品”'],
  },
  download: {
    title: '下載 Coppice',
    intro: '免費，在你自己的電腦上執行，無需 Coppice 帳號。',
    groups: [
      {
        note: '不確定你的 Mac 是哪種？開啓蘋果選單 → 關於本機。如果顯示“晶片”，請選擇 Apple Silicon；如果顯示“處理器”，請選擇 Intel。',
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
        rows: [{ name: 'Windows 11', label: '通過 WSL 安裝' }],
      },
    ],
    checksum: {
      before: '可以用 ',
      sumsLabel: 'SHA256SUMS',
      between: ' 校驗你下載的檔案，具體方法見',
      installLabel: '安裝',
      after: '文件。',
    },
    next: {
      before: '接下來，請按',
      label: '安裝步驟',
      after: '操作。',
    },
  },
};
