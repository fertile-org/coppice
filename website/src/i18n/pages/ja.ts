import type { Pages } from './en';

export const locale = 'ja';

/** 日本語 homepage and download copy. Keys mirror `./en.ts`; anything missing stays English. */
export const pages: Pages = {
  home: {
    title: 'Coppice：マルチエージェント開発のデスクトップアプリ',
    description:
      'Coppice はマルチエージェント開発のデスクトップアプリです。計画を承認し、diff を受け入れます。Claude Code や Codex などがパソコンで動きます。',
    worksWith: '対応 CLI：',
    h1: '専門家を雇い、最終判断はあなたに。',
    lede: 'マルチエージェント開発のデスクトップアプリです。PM エージェントが計画を下書きし、あなたが承認します。専門のエージェントが並行して作ります。あなたが diff を承認するまで、何も完了にはなりません。',
    hero: {
      stillAlt: 'acme-web リポジトリのチケットが並ぶ Coppice のボード',
      videoAlt:
        'Coppice：チケットを書き、デザイン案を比べ、PM エージェントが下書きした計画を承認し、エージェントに作らせてテストさせ、diff にコメントします。あなたが承認するまで、何も完了にはなりません。',
    },
    support: 'オープンソース（Apache 2.0）。ローカルで動き、Coppice アカウントは不要です。',
    downloads: {
      mac: 'macOS 版をダウンロード（Apple Silicon）',
      linux: 'Linux 版をダウンロード（.deb）',
      docs: 'ドキュメントを読む',
    },
    howTitle: '使い方',
    steps: [
      {
        title: 'PM エージェントと計画する',
        body: 'PM エージェントがチケットを読み、計画を下書きします。方針、手順、変更の確認方法です。コードを書き始める前に、あなたがその計画を承認します。',
        imageAlt:
          'ダークモード切り替えの Plan Review。PM エージェントが担当し、提案された計画に方針と手順が並んでいます。',
      },
      {
        title: 'デザインを承認する',
        body: 'チケットが画面の見た目を変えるとき、エージェントはいくつかのレイアウトを見せ、おすすめを示します。あなたが一つを選ぶと、計画はその選択に沿います。',
        imageAlt: 'Settings のテーマ切り替えのデザイン案。3 つのレイアウトと、おすすめのプレビューがあります。',
      },
      {
        title: 'エージェントに作らせる',
        body: '専門のエージェントがチケットを作り、レビューし、テストします。チケットごとに専用の git worktree があるので、並行作業はメインのチェックアウトに触れません。実行中の様子も追えます。',
        imageAlt: 'ボード。進行中、レビュー中、QA、人の確認待ちのチケットがあります。',
      },
      {
        title: 'コードの変更をレビューする',
        body: 'アプリで diff を読み、どの行にもコメントできます。あなたが承認するまで、何も完了にはなりません。Accept と Merge は別の操作です。',
        imageAlt: 'テーマ切り替えのコードレビュー。承認済みの計画、変更されたファイル、diff の 1 行へのコメントです。',
      },
    ],
    howClose: 'ダウンロードして、開いて、作業するだけです。準備は要りません。Docker も Coppice アカウントも不要です。',
    whyTitle: 'Coppice を選ぶ理由',
    reasons: [
      {
        title: '役割ごとに、ふさわしいモデルを。',
        body: 'エージェントごとに CLI とモデルを選べます。計画とレビューにはいちばん強いモデルを、コーディングには速いモデルを使います。すべて 1 つのボードの上です。',
      },
      {
        title: 'ターミナルを行き来するのはもう終わり。',
        body: 'すべてのエージェントの作業が 1 つのボードのカードになります。ブロックされたチケットやレビュー待ちの作業が目立つので、どこで自分が必要かが分かります。',
      },
      {
        title: '取り込まれる前にミスを見つける。',
        body: 'すべての変更は、アプリの中の diff としてあなたを待ちます。あなたが承認するまで何も完了にはならず、承認した内容がそのままマージされます。',
      },
      {
        title: '2 つ目のサブスクリプションは不要。',
        body: 'Coppice は、あなたがすでに契約している CLI を動かします：Claude Code、Codex、Cursor、OpenCode、Kilo Code、GitHub Copilot、Qwen Code、Kimi Code、Grok。',
      },
    ],
    notTitle: 'Coppice ではないもの',
    notItems: [
      'クラウドのコーディングサービスではない',
      'Jira の代わりではない',
      '「エージェントに伝えれば完成品が届く」ものではない',
    ],
  },
  download: {
    title: 'Coppice をダウンロード',
    metaTitle: 'Coppice をダウンロード（macOS・Linux）',
    description:
      '無料でオープンソースのマルチエージェント開発デスクトップアプリ Coppice を、macOS（Apple Silicon・Intel）と Linux 向けに配布しています。アカウント不要。',
    intro: '無料で、あなたのマシンで動きます。Coppice アカウントは不要です。',
    groups: [
      {
        note: 'どの Mac か分からない場合は、Apple メニュー → このMacについて を開いてください。「チップ」と表示されていれば Apple Silicon を、「プロセッサ」と表示されていれば Intel を選びます。',
        rows: [
          { name: 'macOS (Apple Silicon)', label: '.dmg をダウンロード' },
          { name: 'macOS (Intel)', label: '.dmg をダウンロード' },
        ],
      },
      {
        rows: [
          { name: 'Linux (x64)', label: '.deb をダウンロード' },
          { name: 'Linux (ARM64)', label: '.deb をダウンロード' },
        ],
      },
      {
        rows: [{ name: 'Windows 11', label: 'WSL でインストール' }],
      },
    ],
    checksum: {
      before: 'ダウンロードしたファイルは ',
      sumsLabel: 'SHA256SUMS',
      between: ' と照合して確認できます。方法は',
      installLabel: 'インストール',
      after: 'を見てください。',
    },
    next: {
      before: '次に、',
      label: 'インストールの手順',
      after: 'に進んでください。',
    },
  },
};
