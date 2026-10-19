---
title: キーボードショートカット
description: "ボード、チケット、レビューを、キーボードで操作します。"
---

# キーボードショートカット

どこでも `?` を押すと、すべてのショートカットが表示されます。ショートカットは **Navigate**、**Tickets**、**Review** に分かれています。ショートカットは Settings で変更できます。

`Mod` は macOS では `⌘`、Linux と WSL では `Ctrl` です。連続したキーは `G then B` のように表示されます。

入力欄で文字を打っている間はショートカットは動かず、ターミナルは独自のキーを使います。開いているものを閉じるには `Esc` を押します。

<a id="navigate"></a>

## 移動（Navigate）

| 操作 | ID | キー |
| --- | --- | --- |
| コマンドパレットを開く | `palette.open` | `Mod+K` |
| キーボードショートカットを表示 | `help.show` | `?` |
| 閉じる | `close` | `Esc` |
| 次のカード | `board.nextCard` | `J` or `Down` |
| 前のカード | `board.previousCard` | `K` or `Up` |
| 次の列 | `board.nextColumn` | `L` or `Right` |
| 前の列 | `board.previousColumn` | `H` or `Left` |
| 1 列目へ | `board.column.1` | `G then 1` |
| 2 列目へ | `board.column.2` | `G then 2` |
| 3 列目へ | `board.column.3` | `G then 3` |
| 4 列目へ | `board.column.4` | `G then 4` |
| 5 列目へ | `board.column.5` | `G then 5` |
| 6 列目へ | `board.column.6` | `G then 6` |
| 7 列目へ | `board.column.7` | `G then 7` |
| 8 列目へ | `board.column.8` | `G then 8` |
| 9 列目へ | `board.column.9` | `G then 9` |
| Boards へ | `nav.boards` | `G then B` |
| Agents へ | `nav.agents` | `G then A` |
| Chat へ | `nav.chat` | `G then C` |
| Knowledge へ | `nav.knowledge` | `G then K` |
| Repositories へ | `nav.repositories` | `G then R` |
| Plugins へ | `nav.plugins` | `G then P` |
| Users へ | `nav.users` | `G then U` |
| System へ | `nav.tools` | `G then T` |
| Settings へ | `nav.settings` | `G then S` |
| 次のセクション | `drawer.nextSection` | `]` |
| 前のセクション | `drawer.previousSection` | `[` |

1〜9 列目へのショートカットは、ボードの左から右の順です：1 Backlog、2 Plan Review、3 Ready、4 In Progress、5 In Review、6 In QA、7 Wait for Human Review、8 Done、9 Blocked。

Plugins、Users、System、Settings への移動は、サイドバーと同じくメンバーには表示されません。デスクトップアプリには Users への移動もありません。

次のカードと前のカードは列の中で動き、端で折り返しません。次の列と前の列は、同じ位置のカード、最後のカード、または列が空なら列の見出しに移ります。次のセクションと前のセクションは、開いているドロワーの Detail、Live Console、Agent Runs の間を移ります。

<a id="tickets"></a>

## チケット（Tickets）

| 操作 | ID | キー | 条件 |
| --- | --- | --- | --- |
| チケットを開く | `ticket.open` | `Enter` | カードにフォーカスがある |
| チケットを作る | `ticket.create` | `C` | ボード上、ドロワーなし |
| チケットを割り当てる | `ticket.assign` | `A` | カードにフォーカスがある、またはドロワーが開いている |
| 次の列へ移す | `ticket.moveNext` | `M then L` | カードにフォーカスがある、またはドロワーが開いている |
| 前の列へ移す | `ticket.movePrevious` | `M then H` | カードにフォーカスがある、またはドロワーが開いている |
| コメントを投稿 | `ticket.comment` | `Mod+Enter` | コメントエディターにフォーカスがあり、文字が入っている |

チケットを作るショートカットは、フォーカスのある列の **Add ticket…** 欄にフォーカスを移します。Enter でフォームを送信します。この欄は Backlog にあります。

チケットを割り当てるショートカットは、担当者のコンボボックスを開きます。ドロワーが閉じていれば、チケットを開いてからコンボボックスを開きます。

次の列へ移す、前の列へ移すは、上の列の順番に従います。

<a id="review"></a>

## レビュー（Review）

| 操作 | ID | キー | 条件 |
| --- | --- | --- | --- |
| 計画を承認 | `review.approvePlan` | `R then P` | チケットが Plan Review にあり、**Approve plan** が押せる |
| Accept… | `review.accept` | `Mod+Shift+Enter` | チケットが Wait for Human Review にあり、アーカイブされていない |
| Merge… | `review.merge` | `R then M` | **Merge…** がすでに表示されている |
| コードレビューを開く | `review.openCode` | `D` | **Open code review** が押せる |

**Accept…** は **Accept this ticket?** ダイアログを開き、**Cancel** にフォーカスを置きます。終えるには **Accept** を選びます。

**Merge…** は **Merge ticket branch** ダイアログを開きます。

<a id="a-pass-with-only-the-keyboard"></a>

## キーボードだけで一通り

1. ボードで `C` を押します。**Add ticket…** にフォーカスが移ります。タイトルを入力して Enter を押します。
2. `J` か `K` でそのカードに移り、`A` を押します。担当者のコンボボックスが開きます。
3. Enter を押します。
4. コメントエディターにフォーカスを移し、入力して `Mod+Enter` を押します。
5. `D` を押します。
6. チケットが Wait for Human Review にあるときに `Mod+Shift+Enter` を押します。**Cancel** にフォーカスがあります。終えるには **Accept** を選びます。
7. `Esc` を押します。フォーカスはボードのカードに戻ります。

<a id="custom-shortcuts"></a>

## ショートカットを変える

Settings の `[keybindings]` で、どのショートカットも変更できます。上の表のコマンド ID を使ってください。あなたのキーが既定のキーに置き換わり、空の値にするとショートカットがオフになります。

ショートカットが読めない、存在しないコマンドを指している、ほかのコマンドがすでに使っているキーを使う、ブラウザーやシステムのキーである、という場合、そのコマンドは既定のキーのままになります。理由は Settings に表示されます。ショートカットのヘルプとコマンドパレットには、実際に有効なキーが表示されます。

```toml
[keybindings]
"palette.open" = "P"
"ticket.create" = ""
```

**Settings** を開いてファイルを編集し、**Save** を選びます。新しいキーは再起動なしでショートカットのヘルプとコマンドパレットに表示されます。
