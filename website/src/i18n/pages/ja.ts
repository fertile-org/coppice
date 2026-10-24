import type { Pages } from './en';

export const locale = 'ja';

/** 日本語 homepage and download copy. Keys mirror `./en.ts`; anything missing stays English. */
export const pages: Pages = {
  home: {
    title: 'Coppice：オープンソースの AI コーディングエージェントボード',
    description:
      'Coppice はローカルで動くオープンソースの AI コーディングエージェントボード。Claude Code、Codex、Cursor などが並行して計画・実装・レビュー。diff はあなたが承認。',
    worksWith: '対応 CLI：',
    h1: '専門家を雇い、最終判断はあなたに。',
    lede: 'PM エージェントが計画を下書きし、あなたが承認します。専門のエージェントが並行して作ります。あなたが diff を承認するまで、何も完了にはなりません。',
    hero: {
      stillAlt: 'acme-web リポジトリのチケットが並ぶ Coppice のボード',
      videoAlt:
        'Coppice：チケットを書き、PM エージェントが下書きした計画を承認し、エージェントに作らせてテストさせ、diff にコメントします。あなたが承認するまで、何も完了にはなりません。',
      captions: [
        'チケットを書く。',
        'PM エージェントが計画を下書きする。',
        'あなたが承認する。',
        'エージェントが作り、レビューし、テストする。',
        'diff をレビューし、どの行にもコメントできる。',
        'あなたが承認するまで、何も完了しない。',
      ],
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
        title: '作業を任せる。',
        body: 'チケットを書くか、エージェントが提案したチケットを承認します。PM エージェントに割り当てると計画を下書きし、誰かがコードを書く前にあなたが承認します。',
      },
      {
        title: 'エージェントに作らせる。',
        body: 'チケットごとに専用の git worktree があるので、並行して動くエージェントがメインのチェックアウトに触れることはありません。どの実行もライブで追えます。',
      },
      {
        title: 'Human Review。',
        body: 'アプリで diff を読み、どの行にもコメントできます。あなたが承認するまで、何も完了にはなりません。',
      },
    ],
    howClose: 'ダウンロードして、開いて、作業するだけ。準備は要りません。Docker も Coppice アカウントも不要です。',
    whyTitle: 'Coppice を選ぶ理由',
    reasons: [
      {
        title: '役割ごとに、ふさわしいモデルを。',
        body: 'エージェントごとに CLI とモデルを選べます。計画とレビューはいちばん強いモデルで、コーディングは速いモデルで。すべて 1 つのボードの上で。',
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
      '「エージェントに言えば完成品が届く」ものではない',
    ],
  },
  download: {
    title: 'Coppice をダウンロード',
    metaTitle: 'Coppice をダウンロード（macOS・Linux）',
    description:
      '無料でオープンソースの AI コーディングエージェントボード Coppice を、macOS（Apple Silicon・Intel）と Linux 向けに配布しています。アカウント不要。',
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
      between: ' で確認できます。方法は',
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
