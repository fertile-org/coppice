# Custom key bindings — design

**Status:** spec for review. Tracks [#49](https://github.com/fertile-org/coppice/issues/49), the custom-bindings slice. This change is the spec only.

This replaces the Settings recorder in [Keyboard-first Coppice](2026-10-15-keyboard-first-design.md) (the **Record shortcut** / **Reset** section and build step 4). Defaults, scopes, the palette, and the help overlay stay as that spec defines them. Customization is a `[keybindings]` table in `config.toml`.

## Why

People rebind Coppice from the same file Settings already edits. The help overlay and the command palette show the keys that are actually in effect. A shortcut Coppice cannot use leaves that command on its default keys and explains why.

## Decisions

| Topic | Decision |
|---|---|
| Where | `[keybindings]` in `config.toml`. The Settings editor saves the file and keeps comments. |
| What can change | Every command id in `config/default-keybindings.toml`. An absent id keeps its default. |
| Empty value | `""` turns that command's shortcut off. The keys it held become free for a later line. |
| Bad entry | That command keeps its default keys. The rest of the table still applies. The file is still saved when the rest of `config.toml` is valid. Coppice still starts. |
| Whole file | A file that is not valid TOML fails to load and Save leaves it unchanged, same as any other syntax error. |
| Display | The help overlay and the palette show the effective keys. `Mod` is `⌘` on macOS and `Ctrl` on Linux and WSL. |
| Apply | Save updates the open app. Shortcut changes do not ask for a restart. |
| UI | The existing Settings editor and its status line. A warning list under the editor when any entry was skipped. |
| Menu | Electron menu accelerators are a later slice. They use the effective keys from this spec. |

## Format

The section is optional. Each key is a command id in quotes. The value is one string, or a list of strings. ` or ` inside a string means several bindings for that one command. Comments stay where the user put them.

```toml
# Keyboard shortcuts. Your keys replace the defaults, and an empty value turns one off.
# Mod is Cmd on macOS and Ctrl on Linux and WSL.
# A shortcut Coppice can't use is skipped, and that command keeps its default.
# [keybindings]
# "palette.open" = "Mod+K"

[keybindings]
"palette.open" = "Mod+P"
"ticket.create" = ""
"board.nextCard" = ["J", "Down"]
"nav.boards" = "G then B"
```

A chord is modifiers, then `+`, then a key. Modifiers are `Mod`, `Shift`, `Alt`, and `Option`. Each modifier appears once. `Option` is `Alt`. `Mod` is Cmd on macOS and Ctrl on Linux and WSL.

A sequence is two keys with ` then ` between them. Each key is one letter or one digit. A sequence has no modifiers.

Chord keys are a letter, a digit, `?`, `[`, `]`, `Esc` (`Escape`), `Enter` (`Return`), `Up`, `Down`, `Left`, `Right` (`ArrowUp` and the other arrow names), `Space`, and `Tab`.

Letter case is ignored. `shift+c` and `Shift+C` are the same binding as `C`: Shift on a bare letter is dropped. `g then b` is `G then B`.

`Ctrl`, `Cmd`, `Meta`, `Super`, and `Win` are not modifier names. `Ctrl+K` is a shortcut Coppice cannot read.

The commented sample above is what `config.example.toml` and a new desktop `config.toml` already lead with, plus the third comment line.

## Commands that can be changed

These ids match the registry and `config/default-keybindings.toml`. The action names are the help-overlay labels.

### Navigate

| Action | Id | Default |
|---|---|---|
| Open command palette | `palette.open` | `Mod+K` |
| Show keyboard shortcuts | `help.show` | `?` |
| Close | `close` | `Esc` |
| Next card | `board.nextCard` | `J` or `Down` |
| Previous card | `board.previousCard` | `K` or `Up` |
| Next column | `board.nextColumn` | `L` or `Right` |
| Previous column | `board.previousColumn` | `H` or `Left` |
| Go to column 1 … Go to column 9 | `board.column.1` … `board.column.9` | `G then 1` … `G then 9` |
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

### Tickets

| Action | Id | Default |
|---|---|---|
| Open ticket | `ticket.open` | `Enter` |
| Create ticket | `ticket.create` | `C` |
| Assign ticket | `ticket.assign` | `A` |
| Move to next column | `ticket.moveNext` | `M then L` |
| Move to previous column | `ticket.movePrevious` | `M then H` |
| Post comment | `ticket.comment` | `Mod+Enter` |

### Review

| Action | Id | Default |
|---|---|---|
| Approve plan | `review.approvePlan` | `R then P` |
| Accept… | `review.accept` | `Mod+Shift+Enter` |
| Merge… | `review.merge` | `R then M` |
| Open code review | `review.openCode` | `D` |

An id missing from this list is unknown. Coppice skips that line. Scopes, labels, and which commands exist stay in the registry. The table cannot add a command.

## How a table is applied

Lines apply from top to bottom, on top of the defaults.

1. An unknown command id is skipped. Nothing else about that line is checked.
2. A value Coppice cannot read is skipped. The command keeps every default key. One bad item in a list skips the whole value.
3. A reserved chord is skipped. The command keeps its defaults.
4. A binding already held by another command is skipped. The command keeps its defaults. A repeated key inside the same value is kept once.
5. Any other value replaces that command's keys. `""` and `[]` leave it with none.

Skipping a line leaves the keys that command already had, including a default. A later line can take a key that an earlier line released:

```toml
[keybindings]
"palette.open" = ""
"ticket.create" = "Mod+K"
```

`Mod+K` is free because **Open command palette** was turned off first. The same pair in the other order skips **Create ticket**, because `Mod+K` is still held by **Open command palette**.

A plain chord (no `Mod` and no `Alt`) whose key is the first key of a sequence still in effect clashes with that sequence. `G` clashes with **Go to column 1** while any `G then …` binding remains. `Mod+G` does not. A sequence whose first key is still a plain chord clashes the same way.

The command that keeps the key is the one already in effect. When several sequences share a first key, the warning names the first of them in the table above.

Two commands in the table never both keep the same binding. The earlier line keeps it.

## Reserved chords

These chords are skipped. `Mod+K` stays available. Sequences and single keys are available, including `?`, `[`, `]`, `Esc`, and the arrow keys.

| Written as | Held by |
|---|---|
| `Mod+Q` | Quit |
| `Mod+W`, `Mod+T`, `Mod+N` | Close tab, new tab, new window |
| `Mod+R`, `Mod+L` | Reload, address bar |
| `Mod+Shift+T`, `Mod+Shift+W` | Reopen tab, close window |
| `Mod+1` … `Mod+9` | Browser tab numbers |
| `Mod+Shift+[`, `Mod+Shift+]` | Safari previous / next tab |
| `Mod+Alt+Left`, `Mod+Alt+Right`, `Mod+Alt+Up`, `Mod+Alt+Down` | Chrome tab switching on macOS |
| `Alt+Left`, `Alt+Right` | Back / forward |
| `Mod+H`, `Mod+M`, `Mod+Tab`, `Mod+Space` | Hide, minimize, switch app, Spotlight |
| `Mod+C`, `Mod+V`, `Mod+X`, `Mod+A`, `Mod+Z` | Clipboard and undo |
| `Mod+Shift+M` | Chrome profile menu, and Firefox Responsive Design Mode on Linux and Windows |

## When the section itself cannot be read

If `[keybindings]` is present and is not a table, every command keeps its defaults and Coppice warns once: **Couldn't read [keybindings]. Each line needs a command id and a key. All shortcuts use their defaults.**

If a key is a nested table, that entry was an unquoted dotted id such as `palette.open`. That entry is skipped with the quotes warning. Quoted siblings in the same table still apply.

## Help overlay and palette

Both surfaces use the keys in effect after the rules above.

The help overlay keeps the title **Keyboard shortcuts**, the groups **Navigate**, **Tickets**, and **Review**, and the action names in the tables above. **Go to column 1** through **Go to column 9** stay numbered there. Each row's keys are the effective bindings: `⌘+P` or `Ctrl+P`, `G then B`, and `J or Down` when a command has several. A command turned off has a blank key column. The footer stays **Change shortcuts in Settings.** When any entry was skipped, a second footer line is **Some shortcuts in Settings couldn't be used. Those rows show the defaults.**

The palette row shows the same effective keys, with **Go to {column} column** for the nine column jumps. A command with no keys has no key hint on its row. The palette has no extra warning line.

Member and desktop omissions are unchanged. A skipped binding on an omitted command still warns in Settings.

## Settings

Save writes the editor text, including a skipped line, so the user's wording and comments stay put. The status line is **Saved.** when every binding applied. When one or more were skipped, the status line is **Saved. Some shortcuts couldn't be used, so they kept their defaults.** Under the editor, one status list item per skipped entry, in file order. Opening Settings shows that list when the file on disk already has skipped entries.

The list is the last applied file. Typing clears the **Saved.** line the way the editor already does, and the list refreshes on the next successful save.

A TOML syntax error still uses the existing Save failure, naming the line, and writes nothing.

`GET /api/keybindings` returns the effective overrides and the warning strings. An override is present only for a command the table changed, including a command turned off (`[]`). A skipped command is absent, so the client keeps the registry default. Each warning is the sentence in **Strings for Content**, with `{action}` and `{other}` already replaced by action labels.

## Docs

The implementation adds the notes below. No other page changes.

In `website/src/pages/docs/shortcuts.md`, under **Custom shortcuts**, after the sentence that an empty value turns the shortcut off:

> If a shortcut can't be read, names an unknown command, uses a key another command already has, or belongs to the browser or the system, that command keeps its default. Settings explains why. The shortcut help and the command palette show the keys that are in effect.

In `website/src/pages/docs/settings.md`, the `[keybindings]` rejection sentence becomes:

> A shortcut Coppice can't read, an unknown command id, a key another command already has, or a key that belongs to the browser or the system is skipped. That command keeps its default, and Settings explains why.

The example files gain the comment line in **Format**.

## Strings for Content

`{id}` is the command id as written. `{text}` is the shortcut text Coppice could not read, trimmed. `{binding}` is the normalized form (`Mod+K`, `G`, `G then B`). `{action}` is the label of the command that kept its default. `{other}` is the label of the command that kept the key. When several sequences share a first key, `{other}` is the first of them in the tables above.

Settings, after a save that skipped at least one entry:

- Saved. Some shortcuts couldn't be used, so they kept their defaults.

One list item per skipped entry:

- No command called "{id}". No shortcut was changed.
- Couldn't read "{text}". Write it like "Mod+K" or "G then B". {action} keeps its default.
- "{binding}" is already used by {other}. {action} keeps its default.
- "{binding}" belongs to the browser or the system. {action} keeps its default.
- Put each command id in quotes, like "palette.open".

When the section is not a table, the only list item is:

- Couldn't read [keybindings]. Each line needs a command id and a key. All shortcuts use their defaults.

Help overlay, second footer line while any entry is skipped:

- Some shortcuts in Settings couldn't be used. Those rows show the defaults.

Example config comment:

- A shortcut Coppice can't use is skipped, and that command keeps its default.

Docs, Keyboard shortcuts:

- If a shortcut can't be read, names an unknown command, uses a key another command already has, or belongs to the browser or the system, that command keeps its default. Settings explains why. The shortcut help and the command palette show the keys that are in effect.

Docs, Settings:

- A shortcut Coppice can't read, an unknown command id, a key another command already has, or a key that belongs to the browser or the system is skipped. That command keeps its default, and Settings explains why.

Examples a reviewer can read out loud:

- No command called "nope". No shortcut was changed.
- Couldn't read "Ctrl+K". Write it like "Mod+K" or "G then B". Create ticket keeps its default.
- "Mod+K" is already used by Open command palette. Create ticket keeps its default.
- "G" is already used by Go to column 1. Create ticket keeps its default.
- "Mod+W" belongs to the browser or the system. Open command palette keeps its default.

**Saved.**, **Change shortcuts in Settings.**, **Keyboard shortcuts**, and the action names in the tables above stay as they are.

## Done when

- A valid line replaces that command in the help overlay and the palette, with no restart.
- `""` blanks that help row, drops the palette hint, and frees the key for a later line.
- An unknown id, a value Coppice cannot read, a reserved chord, and a clash each leave that command on its default, show the matching sentence, and still apply every valid sibling.
- The saved file still contains the skipped line.
- A TOML syntax error saves nothing.
- The two docs notes and the example comment match the copy above.
