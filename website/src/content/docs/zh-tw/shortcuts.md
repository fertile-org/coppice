---
title: 鍵盤快捷鍵
description: "用鍵盤在 Coppice 中操作：看板、工單和審查。"
---

# 鍵盤快捷鍵

在任意位置按 `?` 即可檢視所有快捷鍵。快捷鍵分為 **Navigate**（導航）、**Tickets**（工單）和 **Review**（審查）三組。可以在 Settings 中修改快捷鍵。

`Mod` 在 macOS 上是 `⌘`，在 Linux 和 WSL 上是 `Ctrl`。按鍵序列寫作 `G then B`（先按 G，再按 B）。

在輸入框中打字時快捷鍵不會觸發，終端也保留自己的按鍵。按 `Esc` 可關閉當前開啟的任何內容。

<a id="navigate"></a>

## Navigate（導航）

| 操作 | Id | 按鍵 |
| --- | --- | --- |
| Open command palette（開啟命令面板） | `palette.open` | `Mod+K` |
| Show keyboard shortcuts（顯示鍵盤快捷鍵） | `help.show` | `?` |
| Close（關閉） | `close` | `Esc` |
| Next card（下一張卡片） | `board.nextCard` | `J` 或 `Down` |
| Previous card（上一張卡片） | `board.previousCard` | `K` 或 `Up` |
| Next column（下一列） | `board.nextColumn` | `L` 或 `Right` |
| Previous column（上一列） | `board.previousColumn` | `H` 或 `Left` |
| Go to column 1（轉到第 1 列） | `board.column.1` | `G then 1` |
| Go to column 2（轉到第 2 列） | `board.column.2` | `G then 2` |
| Go to column 3（轉到第 3 列） | `board.column.3` | `G then 3` |
| Go to column 4（轉到第 4 列） | `board.column.4` | `G then 4` |
| Go to column 5（轉到第 5 列） | `board.column.5` | `G then 5` |
| Go to column 6（轉到第 6 列） | `board.column.6` | `G then 6` |
| Go to column 7（轉到第 7 列） | `board.column.7` | `G then 7` |
| Go to column 8（轉到第 8 列） | `board.column.8` | `G then 8` |
| Go to column 9（轉到第 9 列） | `board.column.9` | `G then 9` |
| Go to Boards（轉到看板） | `nav.boards` | `G then B` |
| Go to Agents（轉到代理） | `nav.agents` | `G then A` |
| Go to Chat（轉到聊天） | `nav.chat` | `G then C` |
| Go to Knowledge（轉到知識庫） | `nav.knowledge` | `G then K` |
| Go to Repositories（轉到倉庫） | `nav.repositories` | `G then R` |
| Go to Plugins（轉到外掛） | `nav.plugins` | `G then P` |
| Go to Users（轉到使用者） | `nav.users` | `G then U` |
| Go to System（轉到系統） | `nav.tools` | `G then T` |
| Go to Settings（轉到設定） | `nav.settings` | `G then S` |
| Next section（下一部分） | `drawer.nextSection` | `]` |
| Previous section（上一部分） | `drawer.previousSection` | `[` |

**Go to column 1** 到 **Go to column 9** 按看板從左到右的順序對應：1 Backlog、2 Plan Review、3 Ready、4 In Progress、5 In Review、6 In QA、7 Wait for Human Review、8 Done、9 Blocked。

和側邊欄一樣，成員（member）看不到 **Go to Plugins**、**Go to Users**、**Go to System** 和 **Go to Settings**。桌面應用中也沒有 **Go to Users**。

**Next card** 和 **Previous card** 在同一列內移動，到頭後不會迴圈。**Next column** 和 **Previous column** 會落到相同序號的卡片上；如果沒有，就落到最後一張卡片；如果該列為空，就落到列標題上。**Next section** 和 **Previous section** 會在已開啟的抽屜面板中依次切換 Detail、Live Console 和 Agent Runs。

<a id="tickets"></a>

## Tickets（工單）

| 操作 | Id | 按鍵 | 生效條件 |
| --- | --- | --- | --- |
| Open ticket（開啟工單） | `ticket.open` | `Enter` | 某張卡片處於焦點 |
| Create ticket（建立工單） | `ticket.create` | `C` | 在看板上，且沒有開啟抽屜面板 |
| Assign ticket（指派工單） | `ticket.assign` | `A` | 某張卡片處於焦點，或抽屜面板已開啟 |
| Move to next column（移到下一列） | `ticket.moveNext` | `M then L` | 某張卡片處於焦點，或抽屜面板已開啟 |
| Move to previous column（移到上一列） | `ticket.movePrevious` | `M then H` | 某張卡片處於焦點，或抽屜面板已開啟 |
| Post comment（發表評論） | `ticket.comment` | `Mod+Enter` | 評論編輯器處於焦點且有內容 |

**Create ticket** 會聚焦到當前列的 **Add ticket…** 輸入框，按 Enter 提交。該輸入框位於 Backlog 列。

**Assign ticket** 會開啟負責人下拉框。如果抽屜面板沒有開啟，會先開啟工單，再開啟下拉框。

**Move to next column** 和 **Move to previous column** 遵循上面的列順序。

<a id="review"></a>

## Review（審查）

| 操作 | Id | 按鍵 | 生效條件 |
| --- | --- | --- | --- |
| Approve plan（批准計劃） | `review.approvePlan` | `R then P` | 工單處於 Plan Review，且 **Approve plan** 可用 |
| Accept…（接受） | `review.accept` | `Mod+Shift+Enter` | 工單處於 Wait for Human Review，且未歸檔 |
| Merge…（合併） | `review.merge` | `R then M` | **Merge…** 已經顯示 |
| Open code review（開啟程式碼審查） | `review.openCode` | `D` | **Open code review** 可用 |

**Accept…** 會開啟 **Accept this ticket?** 對話方塊，焦點在 **Cancel** 上。選擇 **Accept** 即可完成。

**Merge…** 會開啟 **Merge ticket branch** 對話方塊。

<a id="a-pass-with-only-the-keyboard"></a>

## 只用鍵盤走一遍

1. 在看板上按 `C`，焦點會移到 **Add ticket…**。輸入標題，按 Enter。
2. 用 `J` 或 `K` 移到這張卡片，然後按 `A`，負責人下拉框會開啟。
3. 按 Enter。
4. 聚焦評論編輯器，輸入內容，按 `Mod+Enter`。
5. 按 `D`。
6. 工單進入 Wait for Human Review 後，按 `Mod+Shift+Enter`。焦點在 **Cancel** 上，選擇 **Accept** 即可完成。
7. 按 `Esc`，焦點會回到看板上的那張卡片。

<a id="custom-shortcuts"></a>

## 自定義快捷鍵

在 Settings 的 `[keybindings]` 下可以修改任意快捷鍵。使用上面表格中的命令 id。你設定的按鍵會替換預設按鍵，設為空值則關閉該快捷鍵。

如果某個快捷鍵無法解析、指向未知命令、與其他命令的按鍵重複，或者屬於瀏覽器或系統，該命令會保留預設按鍵，Settings 會說明原因。快捷鍵幫助和命令面板顯示的是當前實際生效的按鍵。

```toml
[keybindings]
"palette.open" = "P"
"ticket.create" = ""
```

開啟 **Settings**，編輯檔案，然後選擇 **Save**。新按鍵無需重啟，就會出現在快捷鍵幫助和命令面板中。
