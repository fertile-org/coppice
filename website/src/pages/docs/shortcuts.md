---
layout: ../../layouts/Docs.astro
title: Keyboard shortcuts
description: "Move around Coppice from the keyboard: the board, tickets, and review."
---

# Keyboard shortcuts

Press `?` anywhere to see every shortcut. Shortcuts are grouped as **Navigate**, **Tickets**, and **Review**. Change shortcuts in Settings.

`Mod` is `⌘` on macOS and `Ctrl` on Linux and on WSL. Sequences are shown as `G then B`.

Shortcuts don't fire while you're typing in a field, and the terminal keeps its own keys. Press `Esc` to close whatever is open.

## Navigate

| Action | Id | Keys |
| --- | --- | --- |
| Open command palette | `palette.open` | `Mod+K` |
| Show keyboard shortcuts | `help.show` | `?` |
| Close | `close` | `Esc` |
| Next card | `board.nextCard` | `J` or `Down` |
| Previous card | `board.previousCard` | `K` or `Up` |
| Next column | `board.nextColumn` | `L` or `Right` |
| Previous column | `board.previousColumn` | `H` or `Left` |
| Go to column 1 | `board.column.1` | `G then 1` |
| Go to column 2 | `board.column.2` | `G then 2` |
| Go to column 3 | `board.column.3` | `G then 3` |
| Go to column 4 | `board.column.4` | `G then 4` |
| Go to column 5 | `board.column.5` | `G then 5` |
| Go to column 6 | `board.column.6` | `G then 6` |
| Go to column 7 | `board.column.7` | `G then 7` |
| Go to column 8 | `board.column.8` | `G then 8` |
| Go to column 9 | `board.column.9` | `G then 9` |
| Go to Boards | `nav.boards` | `G then B` |
| Go to Agents | `nav.agents` | `G then A` |
| Go to Chat | `nav.chat` | `G then C` |
| Go to Knowledge | `nav.knowledge` | `G then K` |
| Go to Repositories | `nav.repositories` | `G then R` |
| Go to Plugins | `nav.plugins` | `G then P` |
| Go to Users | `nav.users` | `G then U` |
| Go to Tools | `nav.tools` | `G then T` |
| Go to Settings | `nav.settings` | `G then S` |
| Next section | `drawer.nextSection` | `]` |
| Previous section | `drawer.previousSection` | `[` |

**Go to column 1** through **Go to column 9** follow the board from left to right: 1 Backlog, 2 Plan Review, 3 Ready, 4 In Progress, 5 In Review, 6 In QA, 7 Wait for Human Review, 8 Done, 9 Blocked.

**Go to Plugins**, **Go to Users**, **Go to Tools**, and **Go to Settings** are omitted for members, same as the sidebar. The desktop app also omits **Go to Users**.

**Next card** and **Previous card** move within a column and do not wrap. **Next column** and **Previous column** land on the card at the same index, the last card, or the column heading when that column is empty. **Next section** and **Previous section** move the open drawer across Detail, Live Console, and Agent Runs.

## Tickets

| Action | Id | Keys | When |
| --- | --- | --- | --- |
| Open ticket | `ticket.open` | `Enter` | A card is focused |
| Create ticket | `ticket.create` | `C` | Board, no drawer |
| Assign ticket | `ticket.assign` | `A` | A card is focused or the drawer is open |
| Move to next column | `ticket.moveNext` | `M then L` | A card is focused or the drawer is open |
| Move to previous column | `ticket.movePrevious` | `M then H` | A card is focused or the drawer is open |
| Post comment | `ticket.comment` | `Mod+Enter` | The comment editor is focused and has text |

**Create ticket** focuses the **Add ticket…** field in the focused column. Enter submits that form. That field is on Backlog.

**Assign ticket** opens the assignee combobox. If the drawer is closed, the ticket opens and then the combobox opens.

**Move to next column** and **Move to previous column** follow the column order above.

## Review

| Action | Id | Keys | When |
| --- | --- | --- | --- |
| Approve plan | `review.approvePlan` | `R then P` | The ticket is in Plan Review and **Approve plan** is enabled |
| Accept… | `review.accept` | `Mod+Shift+Enter` | The ticket is in Wait for Human Review and is not archived |
| Merge… | `review.merge` | `R then M` | **Merge…** is already shown |
| Open code review | `review.openCode` | `D` | **Open code review** is enabled |

**Accept…** opens the **Accept this ticket?** dialog with **Cancel** focused. Choose **Accept** to finish.

**Merge…** opens the **Merge ticket branch** dialog.

## A pass with only the keyboard

1. On the board, press `C`. It focuses **Add ticket…**. Type a title and press Enter.
2. Move to that card with `J` or `K`, then press `A`. The assignee combobox opens.
3. Press Enter.
4. Focus the comment editor, type, and press `Mod+Enter`.
5. Press `D`.
6. When the ticket is in Wait for Human Review, press `Mod+Shift+Enter`. **Cancel** is focused. Choose **Accept** to finish.
7. Press `Esc`. Focus returns to the card on the board.

## Custom shortcuts

Change any shortcut in Settings, under `[keybindings]`. Use the command's id from the tables above. Your keys replace the defaults, and an empty value turns the shortcut off.

```toml
[keybindings]
"palette.open" = "P"
"ticket.create" = ""
```

Open **Settings**, edit the file, and choose **Save**. The new keys show in the shortcut help and the command palette without a restart.
