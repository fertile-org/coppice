# 日本語技術文章: 出典と適用限界

[SKILL.md](SKILL.md) の出典。機械可読な記録は [source-manifest.json](source-manifest.json)、意味保存の監査は [tests.md](tests.md)、独立審査は [independent-review.md](independent-review.md)。このディレクトリはリポジトリ内の執筆案内であり、marketplace や実行環境の skill ではない。

Coppice 内の文章作成専用。初出が **2020-11-07 より前**の日本語原著技術記事30本を、別のレビュアーが本文と一次の日付・著者表示に照合した。初出範囲 **2003-08-14–2017-11-30**、12筆者ID、12掲載サイト（同じブログ基盤・技術コミュニティの重複あり）。定量的な人気、母語歴、全国的合意、網羅的な認証を意味しない。

2026-11-07（Asia/Saigon）に保持版を審査。現行ページであり、初出時点の固定スナップショットではない。明示的な追記・改稿、翻訳引用、第三者発言、広告は初出時の日本語文体根拠から分ける。不明な黙示的改稿は残る。JA17の2023追記を除外。JA22/23/24は2026/2026/2025更新を明示し、語彙の初出年代を断定しない。JA12には2015/2016/2018の改稿明示がある。

## 30本の出典

| ID | 初出（書誌ラベル） | 筆者 | 出典 | 読み取った論点・限定 |
|---|---|---|---|---|
| JA01 | 2009-08-22 | 結城浩 | [Gitの基礎練習](https://www.hyuki.com/techinfo/gitinit) | 操作と結果、未commitの状態 |
| JA02 | 2004-03-08 | 結城浩 | [バッドノウハウからグッドラッパーへ](https://www.hyuki.com/techinfo/knowhow) | ラッパー作成者／利用者と留保 |
| JA03 | 2003-08-14 | 結城浩 | [固有IDのシンプル・シナリオ](https://www.hyuki.com/techinfo/uniqid) | 主体の交代、ID一致と推測の範囲 |
| JA04 | 2017-04-11 | 徳丸浩 | [teratailに投稿されたメールフォームにCSRF脆弱性が残存した理由 | 徳丸浩の日記](https://blog.tokumaru.org/2017/04/teratailcsrf.html) | 3段階の処理、空または不一致 |
| JA05 | 2017-11-30 | 徳丸浩 | [IEのクッキーモンスターバグはWindows 10で解消されていた | 徳丸浩の日記](https://blog.tokumaru.org/2017/11/ie-cookie-monster-bug-fixed-on-windows-10.html) | 常体要約と敬体本文、検証と予想 |
| JA06 | 2015-12-07 | 徳丸浩 | [WordPressの侵入対策は脆弱性管理とパスワード管理を中心に考えよう | 徳丸浩の日記](https://blog.tokumaru.org/2015/12/wordpress-security.html) | 安全上の条件、対策の優先度 |
| JA07 | 2015-11-14 | 藤吾郎 (gfx) | [AndroidのORMに求めること、あるいはOrmaを開発した話 - Islands in the byte stream](https://gfx.hatenablog.com/entry/2015/11/14/214559) | 要求と未実装。古い測定は訂正あり |
| JA08 | 2015-06-21 | 藤吾郎 (gfx) | [Android DataBinding導入の第一歩: ButterKnife+ViewHolderパターンを置き換える - Islands in the byte stream](https://gfx.hatenablog.com/entry/2015/06/21/124133) | 移行差分、長い比較説明 |
| JA10 | 2015-01-24 | azu | [テストできないコードをE2Eテストを使ってリファクタリングしよう | Web Scratch](https://efcl.info/2015/01/24/testium-e2e-refactoring/) | 前提→設定→テスト→例外 |
| JA11 | 2016-10-23 | azu | [TestCafeでブラウザの自動テスト(E2Eテスト) | Web Scratch](https://efcl.info/2016/10/23/testcafe/) | snapshot、await、未調査の限定 |
| JA12 | 2015-01-09 | azu | [ライブラリをES2015(ES6)で書いて公開する所から始めよう | Web Scratch](https://efcl.info/2015/01/09/write-es6/) | 構文／API、配布と管理の違い。改稿あり |
| JA13 | 2015-02-23 | mattn | [Big Sky :: 開発速度を加速するツール、goemon を書いた。](https://mattn.kaoriya.net/software/lang/go/20150223224545.htm) | 設定例と既定動作 |
| JA14 | 2015-10-30 | mattn | [Big Sky :: golang で UNIX コマンドパイプラインを扱う](https://mattn.kaoriya.net/software/lang/go/20151030131242.htm) | 短いがpipeの寿命の理由と実装例あり |
| JA15 | 2015-01-13 | mattn | [Big Sky :: golang のリファクタリングには gofmt ではなく、gorename を使おう。](https://mattn.kaoriya.net/software/lang/go/20150113141338.htm) | Windows quoting。英語helpは文体根拠外 |
| JA16 | 2015-07-30 | yuuki | [はてなで大規模サービスのインフラを学んだ - ゆううきブログ](https://blog.yuuk.io/entry/large-scale-infrastructure) | 参照のみのscale、非同期の局所定義 |
| JA17 | 2015-05-28 | yuuki | [2015年Webサーバアーキテクチャ序論 - ゆううきブログ](https://blog.yuuk.io/entry/2015-webserver-architecture) | 親子process、順序。2023追記は除外 |
| JA18 | 2015-12-17 | wtatsuru | [はてなで新しくWebサービスを作るときのインフラの作り方 - Hatena Developer Blog](https://developer.hatenastaff.com/entry/building-web-service-2015) | 2015年12月現在の構成と規模条件 |
| JA19 | 2015-03-17 | Songmu | [Goで外部コマンドのtimeoutを管理するやつを書いた | おそらくはそれさえも平凡な日々](https://songmu.jp/riji/entry/2015-03-17-go-timeout.html) | timeout、signal、AND条件 |
| JA20 | 2015-04-18 | Songmu | [コマンドラインツールを作るときに考えているちょっとした設計方針 | おそらくはそれさえも平凡な日々](https://songmu.jp/riji/entry/2015-04-18-commandline-tool-design.html) | CLIとlibrary、場合による設計判断 |
| JA21 | 2016-06-13 | Songmu | [Makefileを自己文書化する `make2help` | おそらくはそれさえも平凡な日々](https://songmu.jp/riji/entry/2016-06-13-make2help.html) | 別実装Go tool。POSTD/英語記事に着想 |
| JA22 | 2016-03-30 | Jxck | [Content Security Policy(CSP) 対応と report-uri.io でのレポート収集 | blog.jxck.io](https://blog.jxck.io/entries/2016-03-30/content-security-policy.html) | Report-Onlyの否定範囲。2026更新 |
| JA23 | 2017-02-13 | Jxck | [CSP Report 収集と実レポートの考察 | blog.jxck.io](https://blog.jxck.io/entries/2017-02-13/csp-report-case-study.html) | 違反≠攻撃、観測の限界。2026更新 |
| JA24 | 2016-12-12 | Jxck | [Foreign Fetch による Micro Service Workers | blog.jxck.io](https://blog.jxck.io/entries/2016-12-12/foreign-fetch.html) | 提案とtrial、origin/scope。2025更新 |
| JA25 | 2015-08-22 | 和田卓人 (t-wada) | [OSS開発の活発さの維持と良いソフトウェア設計の間には緊張関係があるのだろうか? - t-wadaのブログ](https://t-wada.hatenablog.jp/entry/active-oss-development-vs-simplicity) | 問いの再記述と未決の結論 |
| JA26 | 2014-09-22 | 和田卓人 (t-wada) | [RubyKaigi 2014 に参加した - t-wadaのブログ](https://t-wada.hatenablog.jp/entry/rubykaigi2014) | 会議分析。AST/TracePoint比較は本人の考察 |
| JA27 | 2014-04-07 | kazeburo | [dstat + fluentd + Graphite + Grafana でサーバモニタリングする - blog.nomadscafe.jp](https://blog.nomadscafe.jp/2014/04/dstat-fluentd-graphite-grafana.html) | 環境、保持期間と手順。UI略記は限界 |
| JA28 | 2014-07-01 | kazeburo | [Heroku上でProcletを使っていろいろ動かすと便利 - blog.nomadscafe.jp](https://blog.nomadscafe.jp/2014/07/heroku-proclet.html) | worker/app、任意操作、buildpack |
| JA29 | 2013-03-22 | tagomoris | [out_exec_filter 入出力フォーマットの変更による負荷の変化 - たごもりすメモ](https://tagomoris.hatenablog.com/entry/20130322/1363936468) | 否定結果、印象と原因仮説 |
| JA30 | 2013-11-01 | tagomoris | [Norikra v0.1.0 released! - たごもりすメモ](https://tagomoris.hatenablog.com/entry/20131101/1383302454) | 具体queryと互換範囲の脚注 |
| JA32 | 2015-12-12 | mattn | [Big Sky :: golang では for ループの中で defer してはいけない。](https://mattn.kaoriya.net/software/lang/go/20151212021608.htm) | 関数scope、強い題と本文の例外 |

## 筆者の認知を裏づける外部資料

これは閲覧数ランキングではない。書籍の著者クレジット、外部カンファレンスの登壇、出版賞による定性的な裏づけである。後年の認知は初出当日の読者数を証明しない。

- R03: [gfx](https://gihyo.jp/author/%E8%97%A4%E5%90%BE%E9%83%8E)
- R04: [azu](https://www.kadokawa.co.jp/product/302405003509/)
- R05: [mattn and Songmu](https://gihyo.jp/book/2016/978-4-7741-8392-3)
- R06: [yuuki](https://gihyo.jp/news/report/01/yapcasia2015/0000)
- R09: [t-wada](https://www.oreilly.co.jp/books/9784873114798/)
- R10: [kazeburo](https://gihyo.jp/book/2022/978-4-297-12846-3)
- R11: [tagomoris](https://gihyo.jp/book/2022/978-4-297-13109-8)
- R13: [結城浩 award](https://www.mathsoc.jp/activity/public_relations/press_conference/pubprize2014.html)
- R15: [Songmu recognition](https://gihyo.jp/author/%E6%9D%BE%E6%9C%A8%E9%9B%85%E5%B9%B8)
- R16: [Jxck independent publication](https://gihyo.jp/magazine/wdpress/archive/2012/vol71)
- R17: [徳丸浩](https://tokyo.wordcamp.org/2015/speaker/ockeghem/)
- REV-R19: [wtatsuru](https://jawsdays2014.jaws-ug.jp/speaker/t_watanabe/)

JA18のwtatsuruは、Hatenaという媒体だけで有名と判定していない。JAWS DAYS 2014がAWS/Miiverse構成講演の登壇者として掲載し、本人のブログとprofileへ直接リンクしている。外部の技術コミュニティでの活動を示すが、人気の数値や経歴のすべてを独立証明するものではない。

JA21は [POSTDの許諾翻訳](https://postd.cc/auto-documented-makefile/) と [François Zaninottoの英語原著](https://marmelab.com/blog/2016/02/29/auto-documented-makefile.html) を照合した。自己文書化という発想、`help`、`.DEFAULT_GOAL` は共有するが、英語記事はinline commentとshell pipeline、JA21は自作Go tool、直前のcomment、`-all`、別のGo用例を説明する。原案からの着想を独創的な発明に言い換えない。翻訳記事自体は30本に数えない。

## 規則との対応（観察と編集判断を分ける）

- 文体: JA01の実習、JA05の要約／本文、JA16の回顧。記事全体への敬体強制は不適切。
- 省略と主体: JA01の連続操作、JA03の役割、JA17の親子。毎文の主語追加ではなく交代点を確認。
- 修飾・節: JA08の比較、JA19の入れ子、JA24の長い修飾。固定文字数上限なし。
- 名詞化: JA07の要求一覧、JA20の再利用概念、JA03の命題。こと・名詞見出し・受け身の一律禁止なし。
- 時制・確度: JA05の検証と解消時期予想、JA07の未実装、JA29の否定結果。
- 能力・許可・義務: JA03の可能性、JA16の参照限定、JA06の条件付き助言、JA20/32の例外。
- 条件と範囲: JA04のOR、JA19のAND、JA22の非遮断、JA30の互換限定。十分条件を必要条件へ強化しない。
- 用語・literal: JA12のsyntax/API、JA15のquote、JA21のflag。固有の契約値を整文で変えない。
- 説明順序: JA10/11の段階例、JA27の環境、JA29の測定、JA25の未決。全ジャンルに同じ結論型を押し付けない。
- 敬語・表記: JA01の丁寧な実習、JA13/30のくだけた勧誘、JA27の略記。口癖や誤字を再生産しない。

敬体の新規既定案、句読点・英字間隔、README/UI/referenceの定型は**プロジェクト向けの編集判断・転用**であり、資料の一致した規範ではない。UIやreferenceを30件調査したわけではない。意味保存テストは研究者が作った架空例で、出典の引用やCoppice仕様ではない。

完全な原文、HTML、画像、認知証拠ページのコピーは非公開研究領域にのみ保持する。ここには派生した指針・短い根拠要約・出典だけを置く。原文コードは歴史的資料であり、実行済み・現在安全・Coppiceに存在する機能とは主張しない。JA09は内容の薄さ、JA31は未取得のため不算入。今回の独立審査で資料の差し替えはない。
