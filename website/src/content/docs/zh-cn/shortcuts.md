---
title: 键盘快捷键
metaTitle: "键盘快捷键"
description: "用键盘操作 Coppice：在看板上移动，打开和移动工单，不用鼠标就能审核 diff，还能按自己的习惯自定义快捷键。"
---

# 键盘快捷键

在任意位置按 `?` 即可查看所有快捷键。快捷键分为 **Navigate**（导航）、**Tickets**（工单）和 **Review**（审核）三组。可以在 Settings 中修改快捷键。

`Mod` 在 macOS 上是 `⌘`，在 Linux 和 WSL 上是 `Ctrl`。按键序列写作 `G then B`（先按 G，再按 B）。

在输入框中打字时快捷键不会触发，终端也保留自己的按键。按 `Esc` 可关闭当前打开的任何内容。

<a id="navigate"></a>

## Navigate（导航）

| 操作 | Id | 按键 |
| --- | --- | --- |
| Open command palette（打开命令面板） | `palette.open` | `Mod+K` |
| Show keyboard shortcuts（显示键盘快捷键） | `help.show` | `?` |
| Close（关闭） | `close` | `Esc` |
| Next card（下一张卡片） | `board.nextCard` | `J` 或 `Down` |
| Previous card（上一张卡片） | `board.previousCard` | `K` 或 `Up` |
| Next column（下一列） | `board.nextColumn` | `L` 或 `Right` |
| Previous column（上一列） | `board.previousColumn` | `H` 或 `Left` |
| Go to column 1（转到第 1 列） | `board.column.1` | `G then 1` |
| Go to column 2（转到第 2 列） | `board.column.2` | `G then 2` |
| Go to column 3（转到第 3 列） | `board.column.3` | `G then 3` |
| Go to column 4（转到第 4 列） | `board.column.4` | `G then 4` |
| Go to column 5（转到第 5 列） | `board.column.5` | `G then 5` |
| Go to column 6（转到第 6 列） | `board.column.6` | `G then 6` |
| Go to column 7（转到第 7 列） | `board.column.7` | `G then 7` |
| Go to column 8（转到第 8 列） | `board.column.8` | `G then 8` |
| Go to column 9（转到第 9 列） | `board.column.9` | `G then 9` |
| Go to Boards（转到看板） | `nav.boards` | `G then B` |
| Go to Agents（转到智能体） | `nav.agents` | `G then A` |
| Go to Chat（转到聊天） | `nav.chat` | `G then C` |
| Go to Knowledge（转到知识库） | `nav.knowledge` | `G then K` |
| Go to Repositories（转到仓库） | `nav.repositories` | `G then R` |
| Go to Plugins（转到插件） | `nav.plugins` | `G then P` |
| Go to Users（转到用户） | `nav.users` | `G then U` |
| Go to System（转到系统） | `nav.tools` | `G then T` |
| Go to Settings（转到设置） | `nav.settings` | `G then S` |
| Next section（下一部分） | `drawer.nextSection` | `]` |
| Previous section（上一部分） | `drawer.previousSection` | `[` |

**Go to column 1** 到 **Go to column 9** 按看板从左到右的顺序对应：1 Backlog、2 Plan Review、3 Ready、4 In Progress、5 In Review、6 In QA、7 Wait for Human Review、8 Done、9 Blocked。

和侧边栏一样，成员（member）看不到 **Go to Plugins**、**Go to Users**、**Go to System** 和 **Go to Settings**。桌面应用中也没有 **Go to Users**。

**Next card** 和 **Previous card** 在同一列内移动，到头后不会循环。**Next column** 和 **Previous column** 会落到相同序号的卡片上；如果没有，就落到最后一张卡片；如果该列为空，就落到列标题上。**Next section** 和 **Previous section** 会在已打开的抽屉面板中依次切换 Detail、Live Console 和 Agent Runs。

<a id="tickets"></a>

## Tickets（工单）

| 操作 | Id | 按键 | 生效条件 |
| --- | --- | --- | --- |
| Open ticket（打开工单） | `ticket.open` | `Enter` | 某张卡片处于焦点 |
| Create ticket（创建工单） | `ticket.create` | `C` | 在看板上，且没有打开抽屉面板 |
| Assign ticket（指派工单） | `ticket.assign` | `A` | 某张卡片处于焦点，或抽屉面板已打开 |
| Move to next column（移到下一列） | `ticket.moveNext` | `M then L` | 某张卡片处于焦点，或抽屉面板已打开 |
| Move to previous column（移到上一列） | `ticket.movePrevious` | `M then H` | 某张卡片处于焦点，或抽屉面板已打开 |
| Post comment（发表评论） | `ticket.comment` | `Mod+Enter` | 评论编辑器处于焦点且有内容 |

**Create ticket** 会聚焦到当前列的 **Add ticket…** 输入框，按 Enter 提交。该输入框位于 Backlog 列。

**Assign ticket** 会打开负责人下拉框。如果抽屉面板没有打开，会先打开工单，再打开下拉框。

**Move to next column** 和 **Move to previous column** 遵循上面的列顺序。

<a id="review"></a>

## Review（审核）

| 操作 | Id | 按键 | 生效条件 |
| --- | --- | --- | --- |
| Approve plan（批准计划） | `review.approvePlan` | `R then P` | 工单处于 Plan Review，且 **Approve plan** 可用 |
| Accept…（接受） | `review.accept` | `Mod+Shift+Enter` | 工单处于 Wait for Human Review，且未归档 |
| Merge…（合并） | `review.merge` | `R then M` | **Merge…** 已经显示 |
| Open code review（打开代码审核） | `review.openCode` | `D` | **Open code review** 可用 |

**Accept…** 会打开 **Accept this ticket?** 对话框，焦点在 **Cancel** 上。选择 **Accept** 即可完成。

**Merge…** 会打开 **Merge ticket branch** 对话框。

<a id="a-pass-with-only-the-keyboard"></a>

## 只用键盘走一遍

1. 在看板上按 `C`，焦点会移到 **Add ticket…**。输入标题，按 Enter。
2. 用 `J` 或 `K` 移到这张卡片，然后按 `A`，负责人下拉框会打开。
3. 按 Enter。
4. 聚焦评论编辑器，输入内容，按 `Mod+Enter`。
5. 按 `D`。
6. 工单进入 Wait for Human Review 后，按 `Mod+Shift+Enter`。焦点在 **Cancel** 上，选择 **Accept** 即可完成。
7. 按 `Esc`，焦点会回到看板上的那张卡片。

<a id="custom-shortcuts"></a>

## 自定义快捷键

在 Settings 的 `[keybindings]` 下可以修改任意快捷键。使用上面表格中的命令 id。你设置的按键会替换默认按键，设为空值则关闭该快捷键。

如果某个快捷键无法解析、指向未知命令、与其他命令的按键重复，或者属于浏览器或系统，该命令会保留默认按键，Settings 会说明原因。快捷键帮助和命令面板显示的是当前实际生效的按键。

```toml
[keybindings]
"palette.open" = "P"
"ticket.create" = ""
```

打开 **Settings**，编辑文件，然后选择 **Save**。新按键无需重启，就会出现在快捷键帮助和命令面板中。
