# Keyboard-first Coppice — design

**Status:** spec for review. Tracks [#49](https://github.com/fertile-org/coppice/issues/49). This change is the spec only.

## Why

The board, ticket drawer, and settings can be used with a pointer, and there is no app-level keymap, command palette, or shortcut help. `cmdk` is already wrapped at `web/src/components/ui/command.tsx`. The board drag sensor is pointer-only (`BoardPage.tsx`). Radix dialogs already trap focus.

One action registry drives the palette, the shortcuts, tooltips, the help overlay, and the Electron menu. The default keys follow Linear and GitHub: `C` create, `J`/`K` move, `G` then a letter to change page, `Mod+K` palette, `?` help.

## Decisions

| Topic | Decision |
|---|---|
| Registry | Each action registers once: `id`, label, default keys, scopes, an enabled-when check, and a handler. Components do not add their own `keydown` listeners. |
| Sequences | v1 includes two-key sequences. A leader waits 1 second, then cancels. An unmatched second key cancels. A match whose action is disabled cancels too, and the second key does not run as its own shortcut. |
| Vim modes | Out of v1. |
| Storage | Overrides live in a `[keybindings]` table in `config.toml`. The Settings editor from #42 saves that file and keeps comments. Defaults stay in the registry. |
| Search | `Mod+K` is search. There is no separate `/` shortcut. |
| Column moves | `M` is only the move prefix: `M` then `L` / `M` then `H`. A two-key sequence stays off Safari's `⌘⇧[` / `⌘⇧]`, Chrome's `⌘⌥` arrows, and the `Ctrl+Shift+Arrow` chords some WSLg builds drop. |
| Merge… | `R` then `M`. `Mod+Shift+M` is not used. Chrome takes that chord for the profile menu on macOS, Linux, and Windows, and Firefox takes `Ctrl+Shift+M` for Responsive Design Mode on Linux and Windows. See conflicts. The sequence does not start with `M` or `G`. |
| Accept… | `Mod+Shift+Enter` opens a confirm dialog. Cancel is focused. The chord does not accept by itself (#12). |
| Drag sensor | Leave dnd-kit on `PointerSensor`. Column moves call the same status update as a drop. |
| `Mod` | `⌘` on macOS (`metaKey`). `Ctrl` on Linux and on WSL, including a Windows browser pointed at a WSL server (`ctrlKey`). |

Column order, left to right, is the order in `BOARD_COLUMNS`:

1. Backlog
2. Plan Review
3. Ready
4. In Progress
5. In Review
6. In QA
7. Wait for Human Review
8. Done
9. Blocked

## Scopes

| Scope | When it is active | Keys Coppice handles |
|---|---|---|
| Global | Board, ticket, diff, and other pages, including when those scopes are active. Suppressed in a text field or the terminal | Palette, help, `G` sequences, Esc |
| Board | A board route, no drawer, focus not in a field | Card and column movement, create, ticket chords |
| Ticket | Drawer open, focus not in a field or the terminal | Drawer sections, the same ticket chords |
| Text | input, textarea, Tiptap, CodeMirror | Esc, `Mod+K`, and `Mod+Enter` in the comment editor |
| Terminal | xterm focused | Esc only. The terminal keeps every other key, including `Ctrl+K` on Linux and WSL |
| Diff | `/code` | Global keys. The diff view adds none in v1 |

Single-key shortcuts and sequences fire only in Global, Board, and Ticket. Text and terminal swallow them. Esc closes the top layer (palette, help, confirm, dialog, terminal focus, drawer) and then blurs a field. `Mod+Enter` posts the comment when the comment editor is focused.

## Default keymap

`Mod` is drawn as `⌘` on macOS and `Ctrl` on Linux and WSL. Sequences are shown as `G then B`. Palette rows and the help overlay use the action names in this table, each in one group: **Navigate**, **Tickets**, or **Review**.

### Navigate

Focus on the board uses a roving tabindex: one card, or the column heading when that column has no cards, is tabbable. Arrows move it. Keys do not wrap at the ends.

| Action | Keys |
|---|---|
| Open command palette | `Mod+K` |
| Show keyboard shortcuts | `?` |
| Close | `Esc` |
| Next card | `J` or `Down` |
| Previous card | `K` or `Up` |
| Next column | `L` or `Right` |
| Previous column | `H` or `Left` |
| Go to column {n} | `G then 1`–`9` |
| Go to Boards | `G then B` |
| Go to Agents | `G then A` |
| Go to Chat | `G then C` |
| Go to Knowledge | `G then K` |
| Go to Repositories | `G then R` |
| Go to Plugins | `G then P` |
| Go to Users | `G then U` |
| Go to Tools | `G then T` |
| Go to Settings | `G then S` |
| Next section | `]` |
| Previous section | `[` |

`{n}` is the column's place in the order above: 1 Backlog, 2 Plan Review, 3 Ready, 4 In Progress, 5 In Review, 6 In QA, 7 Wait for Human Review, 8 Done, 9 Blocked. The palette and the help overlay list nine rows, **Go to column 1** through **Go to column 9**. Admin **Go to** rows are omitted for members, same as the sidebar. Page jumps are the second key after `G`.

Left and Right land on the card at the same index in the neighboring column, or the last card, or the column heading when the column is empty. **Go to column {n}** focuses the first card of that column, or its heading. **Next section** and **Previous section** move the open drawer across Detail, Live Console, and Agent Runs.

### Tickets

These run on the focused card, or on the open ticket. Disabled actions are omitted from the palette and do nothing from the keyboard.

| Action | Keys | When |
|---|---|---|
| Open ticket | `Enter` | A card is focused |
| Create ticket | `C` | Board, no drawer |
| Assign ticket | `A` | A card is focused or the drawer is open |
| Move to next column | `M then L` | A card is focused or the drawer is open |
| Move to previous column | `M then H` | A card is focused or the drawer is open |
| Post comment | `Mod+Enter` | The comment editor is focused and has text |

`M` starts only a column move. The second key is `L` or `H`. **Create ticket** focuses the **Add ticket…** field in the focused column. Enter submits that form. **Assign ticket** opens the assignee combobox.

Next and previous column follow the order above, through Blocked. The move uses the existing status update. A rejected move shows the same error as a failed drop, and the card stays put. Focus stays on the card.

### Review

| Action | Keys | When |
|---|---|---|
| Approve plan | `P then A` | The ticket is in Plan Review and **Approve plan** is enabled |
| Accept… | `Mod+Shift+Enter` | The ticket is in Wait for Human Review and is not archived |
| Merge… | `R then M` | **Merge…** is already shown |
| Review code | `D` | **Review code** is enabled |

**Approve plan** runs the same action as the button. **Accept…** opens a dialog; **Cancel** is focused, and **Accept** in the dialog completes it. **Merge…** opens the existing **Merge ticket branch** dialog. **Review code** opens the same destination as the button. `R` is a leader only for **Merge…**. It is not the `M` or `G` leader.

Archive, Run Agent, Stop, and Ask for changes stay on screen in the tab order. They have no default shortcut, so they are not help-overlay rows. Archive is in the palette under **Tickets**. Choosing it opens **Archive this ticket?** with the body **This archives the ticket. You can unarchive it later.** **Cancel** is focused. **Archive** confirms.

## Command palette

`Mod+K` toggles it and calls `preventDefault`, including while typing, except when xterm is focused. The dialog title is **Command palette**. The field placeholder is **Search actions, tickets, and agents…**

- An empty field shows **Recent** (up to eight rows), then the actions enabled in the current scope.
- A query fuzzy-matches action labels, ticket titles, ticket numbers, agent names, and pages. No hit shows **No matches**.
- Ticket actions are included when a card is focused or the drawer is open.
- Each action row shows its current keys.
- Arrow keys move the highlight. Enter runs it. Esc closes and restores focus.

## Shortcut help

`?` toggles a dialog titled **Keyboard shortcuts**. The action in the palette and in this overlay is **Show keyboard shortcuts**. Esc closes it. Rows use the action names from the keymap, in three groups only: **Navigate**, **Tickets**, **Review**. Each action is in one group, as listed above. Rows show the keys in effect after any override. The footer is **Change shortcuts in Settings.** Focus is trapped and restored on close.

## Browser, OS, and WSL

The default keymap leaves these keys alone. Recording one of them in Settings shows **That key belongs to the browser or the system.** and does not save.

| Keys | macOS | Linux | WSL |
|---|---|---|---|
| `Mod+Q` | Quit | Quit in many apps | Same as Linux; Windows Alt+F4 still closes the window |
| `Mod+W`, `Mod+T`, `Mod+N` | Close tab, new tab, new window | Same with Ctrl | Same in Windows Chrome and in WSLg |
| `Mod+R`, `Mod+L` | Reload, address bar | Same with Ctrl | Same |
| `Mod+Shift+T`, `Mod+Shift+W` | Reopen tab, close window | Same | Same |
| `Mod+1` … `Mod+9` | Browser tab numbers | Same | Same |
| `Mod+Shift+[` / `]` | Safari previous / next tab | — | — |
| `Mod+Option+Arrow` | Chrome tab switching | — | — |
| `Alt+Left` / `Alt+Right` | Back / forward | Same | Same |
| `Mod+H`, `Mod+M`, `Mod+Tab`, `Mod+Space` | Hide, minimize, switch app, Spotlight | — | The Windows host still takes Alt+Tab, the Windows key, and Alt+F4 while a WSLg window is focused |
| `Mod+C` `V` `X` `A` `Z` | Clipboard and undo | Same | Same |
| `Mod+Shift+M` | Chrome profile menu (`⌘⇧M`) | Chrome profile menu and Firefox Responsive Design Mode (`Ctrl+Shift+M`) | Same as Linux in Windows Chrome and Firefox. The Windows host uses `Win+Shift+M` for a different action, so the clash is the browser |
| `Ctrl+Alt+…` | — | TTY switching | Windows attention sequence as well |
| `Ctrl+K` while xterm is focused | — | Kill to end of line | Same, so the palette does not take it |

`Mod+K` is the one browser chord the defaults reclaim. Chrome and Safari use it for the address bar, and the palette keeps it with `preventDefault`.

`Mod+Shift+M` stays with the browser. Chrome binds it on macOS (`⌘⇧M`), Linux, and Windows (`Ctrl+Shift+M`) to the profile menu, and uses the same chord for device mode while DevTools is focused. Firefox binds `Ctrl+Shift+M` to Responsive Design Mode on Linux and Windows, which covers a Linux browser and a Windows browser pointed at WSL. macOS Firefox uses `⌘⌥M` for that mode, and Chrome still takes `⌘⇧M` there. **Merge…** is **R then M**, in **Review** with **Accept…**, so the binding does not start with `M` or `G` and does not take `Mod+Shift+M`.

## Focus and accessibility

Target WCAG 2.1 AA 2.1.1, 2.1.2, 2.4.3, and 2.4.7.

- The first control on a board page is a skip link, **Skip to board**.
- Focus order after that is the existing sidebar, then columns from Backlog through Blocked, cards top to bottom.
- The focused control uses a 2px accent ring, the same accent as `.field-control:focus-visible`.
- Each column keeps an accessible name equal to its heading: Backlog, Plan Review, Ready, In Progress, In Review, In QA, Wait for Human Review, Done, Blocked.
- A polite live region announces a successful move as **Moved to {column}**, using that column's name.
- Closing the drawer returns focus to the card that opened it.
- Palette, help, and confirm dialogs trap focus and restore it. Esc leaves each of them. The terminal loses focus on Esc to the drawer tabs.
- Nothing captures Tab except those dialogs.

## Settings

The Settings page gains a **Keyboard shortcuts** section above the `config.toml` editor. Each action shows its label, its current keys, **Record shortcut**, and **Reset**. **Reset all shortcuts** clears every override.

Recording shows **Press the new shortcut** and listens for the next chord. Esc cancels and shows **Recording cancelled**. A reserved chord shows **That key belongs to the browser or the system.** A chord already used by another action shows **Already used by {action}.** Neither is saved.

Overrides are written only under `[keybindings]`, through the same save that already keeps comments. When the table is absent, the first save adds this commented sample and leaves the rest of the file alone:

```toml
# Keyboard shortcuts. Keys here override the defaults.
# Mod is Cmd on macOS and Ctrl on Linux and WSL.
# [keybindings]
# palette.open = "Mod+K"
```

The Electron application menu is built from the same registry, so its labels and accelerators match the palette and the help overlay. The menu bar stays hidden. On macOS the system menu bar shows the menu. Standard edit and quit items keep the OS accelerators and are not Coppice actions.

## Strings for Content

New copy. Existing button labels (**Approve plan**, **Accept**, **Merge…**, **Review code**, **Post comment**, **Add ticket…**, **Cancel**) stay as they are. `{column}` is one of Backlog, Plan Review, Ready, In Progress, In Review, In QA, Wait for Human Review, Done, Blocked. `{n}` in **Go to column {n}** is 1–9 in that same order. `{action}` is the other action's label.

- Skip to board
- Command palette
- Open command palette
- Search actions, tickets, and agents…
- Recent
- No matches
- Keyboard shortcuts
- Show keyboard shortcuts
- Close
- Navigate
- Tickets
- Review
- Change shortcuts in Settings.
- Go to Boards
- Go to Agents
- Go to Chat
- Go to Knowledge
- Go to Repositories
- Go to Plugins
- Go to Users
- Go to Tools
- Go to Settings
- Next card
- Previous card
- Next column
- Previous column
- Go to column {n}
- Open ticket
- Create ticket
- Move to next column
- Move to previous column
- Assign ticket
- Next section
- Previous section
- Moved to {column}
- Accept…
- Accept this ticket?
- This accepts the work currently in Wait for Human Review.
- Record shortcut
- Press the new shortcut
- Reset
- Reset all shortcuts
- Recording cancelled
- That key belongs to the browser or the system.
- Already used by {action}.
- Archive this ticket?
- This archives the ticket. You can unarchive it later.

## Build split

Ship the rest as four pull requests, in order:

1. **Registry and board focus.** The action registry, scopes, roving tabindex, `H`/`J`/`K`/`L` and arrows, Enter to open, Esc back to the card, drawer `[` / `]`, the skip link, the focus ring, and the move announcement.
2. **Command palette.** `Mod+K`, fuzzy search, recent rows, context-aware ticket actions, key hints on rows.
3. **Default shortcuts and help.** The remaining default keys, the `?` overlay, and a user docs page at `website/src/pages/docs/shortcuts.md`. Includes the keyboard-only walkthrough: create, assign, open, comment, review code, Accept…, back to the board.
4. **Custom bindings.** The Settings section, conflict warnings, reset, the `[keybindings]` table, and Electron menu accelerators from the registry.
