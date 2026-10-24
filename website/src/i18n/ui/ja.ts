import type { Ui } from '../ui';

export const locale = 'ja';

/** 日本語 UI chrome. Keys mirror `./en.ts`; this replaces the whole object. */
export const ui: Ui = {
  languageLabel: '言語',
  skip: '本文へスキップ',
  nav: {
    docs: 'ドキュメント',
    github: 'GitHub',
    download: 'ダウンロード',
  },
  footer: {
    tagline: '専門家を雇い、最終判断はあなたに。',
    licenseBefore: 'Coppice は ',
    licenseLabel: 'Apache 2.0 ライセンス',
    licenseAfter: 'のオープンソースです。',
    docs: 'ドキュメント',
    install: 'インストール',
    concepts: 'コンセプト',
    providers: 'プロバイダー',
    faq: 'よくある質問',
    github: 'GitHub',
  },
  docs: {
    navLabel: 'ドキュメント',
    breadcrumbLabel: 'パンくずリスト',
    pagerLabel: 'ほかのドキュメント',
    previous: '前へ',
    next: '次へ',
    install: 'インストール',
    gettingStarted: 'はじめに',
    concepts: 'コンセプト',
    providers: 'プロバイダー',
    plugins: 'プラグイン',
    faq: 'よくある質問',
    advanced: '応用',
    settings: '設定',
    shortcuts: 'キーボードショートカット',
    backup: 'データのバックアップ',
  },
  search: {
    placeholder: 'ドキュメントを検索',
    label: 'ドキュメントを検索',
    hint: '/',
    footer: '↑↓ で移動、Enter で開く、Esc で閉じる',
    close: '閉じる',
    noResults: '「{query}」に一致する結果はありません。',
  },
};
