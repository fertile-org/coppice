import type { Ui } from '../ui';

export const locale = 'zh-hk';

/** 繁體中文（香港） UI chrome. Keys mirror `./en.ts`; this replaces the whole object. */
export const ui: Ui = {
  languageLabel: '語言',
  skip: '跳到正文',
  nav: {
    docs: '文件',
    blog: '網誌',
    github: 'GitHub',
    download: '下載',
  },
  footer: {
    tagline: '僱用專家，你來拍板。',
    licenseBefore: 'Coppice 基於 ',
    licenseLabel: 'Apache 2.0 許可證',
    licenseAfter: '開源。',
    docs: '文件',
    install: '安裝',
    concepts: '核心概念',
    providers: '供應商',
    faq: '常見問題',
    blog: '網誌',
    github: 'GitHub',
    sponsor: '贊助',
  },
  docs: {
    navLabel: '文件',
    breadcrumbLabel: '導覽路徑',
    pagerLabel: '更多文件',
    previous: '上一頁',
    next: '下一頁',
    install: '安裝',
    gettingStarted: '快速上手',
    concepts: '核心概念',
    providers: '供應商',
    plugins: '外掛',
    faq: '常見問題',
    advanced: '進階',
    settings: '設定',
    shortcuts: '鍵盤快捷鍵',
    backup: '備份資料',
  },
  search: {
    placeholder: '搜尋文件',
    label: '搜尋文件',
    hint: '/',
    footer: '↑↓ 移動，Enter 開啟，Esc 關閉',
    close: '關閉',
    noResults: '沒有找到與「{query}」相關的結果。',
  },
  blog: {
    description:
      'Coppice 團隊的文章，談 Claude Code、Codex 這類程式設計代理，以及如何把它們用在實際專案上。每篇只回答一個問題。',
    empty: '還沒有發佈的文章。',
    published: '發佈於',
    updated: '更新於',
    name: 'Coppice 網誌',
  },
};
