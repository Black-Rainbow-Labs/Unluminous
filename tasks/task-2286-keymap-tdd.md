# task-2286: choosing the keyboard shortcuts

The ticket: *"In settings we want a new option which allows users to customize keyboard shortcuts for
various actions. Research online for popular approaches on how to accomplish this, ux, etc. Also research
the shortcuts for both vs code and IntelliJ, as we want an option to apply those to the users custom
mapping. Eg if I select IntelliJ, all the IntelliJ keyboard mappings that apply to ours should be
applied. Write a tdd, then fully implement."*

## 1. What other editors do

Three editors were read, because between them they cover the designs people already know.

**VS Code** keeps every binding as a row in `keybindings.json`, each row a key, a command and an optional
`when` expression that limits the binding to a context. The Keyboard Shortcuts editor is a searchable
table of every command with columns for the command, the key, the `when` and the source (`Default`,
`User`, `Extension`). Pressing the pencil on a row opens a box that records the next key press. A key that
is already used is not refused: VS Code shows a count of the other commands on it, and `Show Same
Keybindings` lists them. A user binding is written to the file and wins over the default, and a binding
can be removed by writing the command again with a `-` in front of it. Keymaps from other editors arrive
as extensions (`IntelliJ IDEA Keybindings`, `Sublime Text Keymap`), each one a list of rows with the
`User` source replaced by the extension's.

**IntelliJ** has a `Keymap` page in Settings with a dropdown of whole keymaps (`Windows`, `macOS`,
`Eclipse`, `Visual Studio`, `VSCode`, and others), a tree of every action grouped the way the menus are,
a search box that finds an action by name and a second search that finds it by pressing the keys. The
predefined keymaps cannot be edited: changing one shortcut makes a copy, and the copy's file holds only
the shortcuts that changed, with everything else read from the keymap it was copied from. When a chord is
already used IntelliJ asks whether to remove it from the other actions, and right clicking an action
offers `Reset Shortcuts`.

**Zed** has a `base_keymap` setting (`VSCode`, `JetBrains`, `SublimeText`, `Atom`, `Emacs`, `TextMate`,
`None`) and a `keymap.json` whose bindings are applied on top of the base, so a person's own binding
always wins.

Sources: [VS Code key bindings](https://code.visualstudio.com/docs/configure/keybindings),
[IntelliJ: configure keyboard shortcuts](https://www.jetbrains.com/help/idea/configuring-keyboard-shortcuts.html),
[IntelliJ default Windows keymap](https://www.jetbrains.com/help/idea/reference-keymap-win-default.html),
[Zed key bindings](https://zed.dev/docs/key-bindings.html).

## 2. What Unluminous takes from each

The three agree on the parts that matter and Unluminous takes them:

- **A base keymap plus the person's own changes.** Zed's `base_keymap` and IntelliJ's copied keymap are
  the same idea: a named set of bindings, and a short list of differences from it that is the only part
  written down. Unluminous calls the base the **preset**: `Unluminous` (what ships today), `VS Code` and
  `IntelliJ`. Choosing a preset never throws the person's own changes away. They stay on top of whatever
  preset is chosen, which is Zed's rule and IntelliJ's rule.
- **One searchable list of every action**, labelled the way the menus label it, with the chord beside it.
  The search matches the action's name, the menu it is in and the chord's text, so `ctrl+d` finds what
  `Ctrl+D` does. That is VS Code's table and IntelliJ's two searches in one box.
- **Recording rather than typing.** Pressing the chord button on a row starts recording, and the next key
  press with its modifiers is the new chord. `Escape` stops recording and changes nothing. `Backspace`
  with no modifier removes the chord. Typing a chord's name is still possible, but only on the command
  line, where there is no keyboard to record from.
- **A conflict is resolved, not left behind.** VS Code lets two commands share a key and leaves the
  context to decide. Unluminous has no `when` contexts: every shortcut is a menu entry and the menu
  watcher fires the first entry that matches, so two entries on one chord means one of them can never be
  reached. IntelliJ's answer is the right one here: the chord moves to the action it was given to, and the
  action that had it loses it. The page says which action lost it, in a line under the list.
- **Reset** on a row puts that row back to the preset's chord, and `Reset all` takes every change away.

Not taken, each with a reason:

- **Chord sequences** such as VS Code's `Ctrl+K Ctrl+O`. Every shortcut in Unluminous is one key press,
  the menu watcher matches one press at a time, and macOS key equivalents are one press too. A preset row
  whose chord is a sequence is left out of the preset (§4), and the action keeps its Unluminous chord.
- **Double modifier presses** such as IntelliJ's double `Shift` for Search Everywhere. Same reason.
- **`when` contexts.** See above. An action's menu entry being dimmed already decides when it can fire.
- **Mouse shortcuts.** IntelliJ binds mouse clicks too. The ticket is about the keyboard.
- **A file of its own.** VS Code and Zed keep a separate `keybindings.json`. Unluminous keeps one settings
  file read by one store, and the bindings are lines in it (§6). A second file would be a second reader,
  a second writer and a second thing for `settings reset` to forget.

## 3. What can be bound

**Every menu entry whose action is the same whatever the window is showing.** The menus are already the
list of everything a menu or a shortcut can ask for (`app::actions::menus`), and `action list` is built
by walking them, so the keymap is built the same way. An entry is bindable when:

1. its `Action` reads back as itself from its name with no path (`Action::from_name(name, None)`), which
   leaves out `open-recent`, a branch to switch to, a run configuration by name and the explorer's entries
   about one file, because a shortcut cannot carry the path; and
2. its shortcut is read from the keyboard (`Entry::Item::keyboard`). Cut, Copy and Paste are not: egui
   delivers them as clipboard events, so a chord given to `Copy` would never be pressed, and a chord taken
   from it would still copy. They are listed on the page as fixed, with the reason.

The list of bindable actions is built **once**, from the menus as they are with everything switched on
(`MenuState` with every `applies` flag true, a debugger, a notebook, a realm node), because the page has
to list `Step Over` while no program is being debugged. Each action is listed once, under the first menu
it was found in, with the path of menus that leads to it: `Edit › Duplicate Line`.

Keys that are **not** menu entries stay as they are: the editing keys (arrows, `Home`, `Tab`, `Enter`),
`Alt+Up` and `Alt+Down`, the completion list's keys, the four alignment chords the editing area reads
itself, the terminal's keys and the notebook's command mode. They are listed in §8 as reserved, so the
recorder refuses them with the name of what owns them.

## 4. The presets

A preset is a table of rows, each an action name, a chord for Windows and Linux, and a chord for macOS.
An empty chord means the preset gives that action no chord on that platform. **A preset only says what it
changes**: an action a preset does not name keeps its Unluminous chord, unless the preset has given that
chord to another action, in which case it loses it (§5). So `IntelliJ` gives `Duplicate Line` `Ctrl+D`,
and `Show Diff`, which has `Ctrl+D` in Unluminous, loses it.

`Unluminous` is the empty table: today's chords exactly.

The tables are below. They come from VS Code's default `keybindings.json` and IntelliJ's `Windows` and
`macOS` keymaps, for the actions Unluminous has. Where the reference has a chord sequence or a double
press, the row is absent.

### 4.1 IntelliJ

| Action | Windows and Linux | macOS |
|---|---|---|
| `command-palette` (Find Action) | `Ctrl+Shift+A` | `Cmd+Shift+A` |
| `go-to-file` | `Ctrl+Shift+N` | `Cmd+Shift+O` |
| `new-window` | none | none |
| `settings` | `Ctrl+Alt+S` | `Cmd+,` |
| `find` | `Ctrl+F` | `Cmd+F` |
| `replace` | `Ctrl+R` | `Cmd+R` |
| `find-next` | `F3` | `Cmd+G` |
| `find-previous` | `Shift+F3` | `Cmd+Shift+G` |
| `find-in-files` | `Ctrl+Shift+F` | `Cmd+Shift+F` |
| `go-to-line` | `Ctrl+G` | `Cmd+L` |
| `go-to-definition` | `Ctrl+B` | `Cmd+B` |
| `find-references` | `Alt+F7` | `Option+F7` |
| `rename-symbol` | `Shift+F6` | `Shift+F6` |
| `complete-word` | `Ctrl+Space` | `Ctrl+Space` |
| `navigate-back` | `Ctrl+Alt+Left` | `Cmd+[` |
| `navigate-forward` | `Ctrl+Alt+Right` | `Cmd+]` |
| `toggle-line-comment` | `Ctrl+/` | `Cmd+/` |
| `toggle-block-comment` | `Ctrl+Shift+/` | `Cmd+Option+/` |
| `duplicate-line` | `Ctrl+D` | `Cmd+D` |
| `move-line-up` | `Alt+Shift+Up` | `Option+Shift+Up` |
| `move-line-down` | `Alt+Shift+Down` | `Option+Shift+Down` |
| `join-lines` | `Ctrl+Shift+J` | `Ctrl+Shift+J` |
| `go-to-matching-bracket` | `Ctrl+Shift+M` | `Ctrl+M` |
| `reopen-closed-tab` | none | none |
| `close-tab` | `Ctrl+F4` | `Cmd+W` |
| `next-tab` | `Alt+Right` | `Ctrl+Shift+]` |
| `previous-tab` | `Alt+Left` | `Ctrl+Shift+[` |
| `toggle-explorer` | `Alt+1` | `Cmd+1` |
| `toggle-terminal` | `Alt+F12` | `Option+F12` |
| `toggle-maximised-pane` | `Ctrl+Shift+F12` | `Cmd+Shift+F12` |
| `fold-toggle` | `Ctrl+.` | `Cmd+.` |
| `fold-all` | `Ctrl+Shift+Minus` | `Cmd+Shift+Minus` |
| `run-start` | `Shift+F10` | `Ctrl+R` |
| `run-stop` | `Ctrl+F2` | `Cmd+F2` |
| `debug-start` | `Shift+F9` | `Ctrl+D` |
| `debug-resume` | `F9` | `Cmd+Option+R` |
| `debug-step-over` | `F8` | `F8` |
| `debug-step-into` | `F7` | `F7` |
| `debug-step-out` | `Shift+F8` | `Shift+F8` |
| `debug-run-to-cursor` | `Alt+F9` | `Option+F9` |
| `debug-toggle-breakpoint` | `Ctrl+F8` | `Cmd+F8` |
| `debug-evaluate` | `Alt+F8` | `Option+F8` |
| `git-commit` | `Ctrl+K` | `Cmd+K` |
| `git-push` | `Ctrl+Shift+K` | `Cmd+Shift+K` |
| `git-rollback` | `Ctrl+Alt+Z` | `Cmd+Option+Z` |
| `git-show-diff` | none | none |
| `redo` | `Ctrl+Shift+Z` | `Cmd+Shift+Z` |
| `save` | `Ctrl+S` | `Cmd+S` |
| `quit` | none | `Cmd+Q` |

IntelliJ's Expand All is `Ctrl+Shift` and the keypad's plus. It is left out: plus is the shifted `=`, so
`Ctrl+Shift+Plus` is the same key press as `Ctrl+Plus` and would take Increase Font Size away. A test
checks that no preset takes an Unluminous shortcut away without naming that action itself.

`git-show-diff` is none because IntelliJ's own Windows keymap puts `Ctrl+D` on both Duplicate and Compare,
and leaves which one fires to the context. Unluminous has no context, so Duplicate keeps it.
`new-window` and `reopen-closed-tab` have no chord in IntelliJ, so the preset takes theirs away rather
than leaving Unluminous's chord on an action an IntelliJ user would not expect to have one; `Ctrl+Shift+N`
in particular is IntelliJ's Go to File.

### 4.2 VS Code

| Action | Windows and Linux | macOS |
|---|---|---|
| `command-palette` | `Ctrl+Shift+P` | `Cmd+Shift+P` |
| `go-to-file` | `Ctrl+P` | `Cmd+P` |
| `new-window` | `Ctrl+Shift+N` | `Cmd+Shift+N` |
| `open-file` | `Ctrl+O` | `Cmd+O` |
| `settings` | `Ctrl+,` | `Cmd+,` |
| `save` | `Ctrl+S` | `Cmd+S` |
| `save-as` | `Ctrl+Shift+S` | `Cmd+Shift+S` |
| `close-window` | `Ctrl+Shift+W` | `Cmd+Shift+W` |
| `close-tab` | `Ctrl+W` | `Cmd+W` |
| `reopen-closed-tab` | `Ctrl+Shift+T` | `Cmd+Shift+T` |
| `undo` | `Ctrl+Z` | `Cmd+Z` |
| `redo` | `Ctrl+Y` | `Cmd+Shift+Z` |
| `find` | `Ctrl+F` | `Cmd+F` |
| `replace` | `Ctrl+H` | `Cmd+Option+F` |
| `find-next` | `F3` | `Cmd+G` |
| `find-previous` | `Shift+F3` | `Cmd+Shift+G` |
| `find-in-files` | `Ctrl+Shift+F` | `Cmd+Shift+F` |
| `go-to-line` | `Ctrl+G` | `Ctrl+G` |
| `go-to-definition` | `F12` | `F12` |
| `find-references` | `Shift+F12` | `Shift+F12` |
| `rename-symbol` | `F2` | `F2` |
| `complete-word` | `Ctrl+Space` | `Ctrl+Space` |
| `navigate-back` | `Alt+Left` | `Ctrl+Minus` |
| `navigate-forward` | `Alt+Right` | `Ctrl+Shift+Minus` |
| `toggle-line-comment` | `Ctrl+/` | `Cmd+/` |
| `toggle-block-comment` | `Shift+Alt+A` | `Shift+Option+A` |
| `duplicate-line` | `Shift+Alt+Down` | `Shift+Option+Down` |
| `move-line-up` | `Alt+Up` | `Option+Up` |
| `move-line-down` | `Alt+Down` | `Option+Down` |
| `join-lines` | none | `Ctrl+J` |
| `go-to-matching-bracket` | `Ctrl+Shift+\` | `Cmd+Shift+\` |
| `toggle-explorer` | `Ctrl+B` | `Cmd+B` |
| `toggle-terminal` | `Ctrl+`` ` | `Ctrl+`` ` |
| `next-tab` | `Ctrl+Tab` | `Ctrl+Tab` |
| `previous-tab` | `Ctrl+Shift+Tab` | `Ctrl+Shift+Tab` |
| `split-right` | `Ctrl+\` | `Cmd+\` |
| `increase-font-size` | `Ctrl+Plus` | `Cmd+Plus` |
| `decrease-font-size` | `Ctrl+Minus` | `Cmd+Minus` |
| `view-preview` | `Ctrl+Shift+V` | `Cmd+Shift+V` |
| `fold-toggle` | `Ctrl+Shift+[` | `Cmd+Option+[` |
| `fold-none` | `Ctrl+Shift+]` | `Cmd+Option+]` |
| `run-start` | `Ctrl+F5` | `Ctrl+F5` |
| `debug-start` | `F5` | `F5` |
| `debug-resume` | `F5` | `F5` |
| `debug-stop` | `Shift+F5` | `Shift+F5` |
| `debug-step-over` | `F10` | `F10` |
| `debug-step-into` | `F11` | `F11` |
| `debug-step-out` | `Shift+F11` | `Shift+F11` |
| `debug-toggle-breakpoint` | `F9` | `F9` |
| `quit` | `Ctrl+Q` | `Cmd+Q` |

Two rows give one chord to two actions, `debug-start` and `debug-resume` on `F5`. VS Code does the same
and tells them apart by context: `F5` starts a session when none is running and continues one that has
stopped. Unluminous gets the same result without contexts, because the menu watcher only fires an
**enabled** entry and `Start Debugging` is dimmed while a session is running and `Resume` is dimmed while
none is. So a preset may share a chord between two actions when the pair is in
`keymap::SHARED_BY_STATE`, a list of exactly that pair, and a test checks that every other chord in every
preset is used once.

`Ctrl+Minus` on macOS is VS Code's Go Back, and `Cmd+Minus` is the font size, which are two different
chords because on macOS `Ctrl` and `Cmd` are two keys (§5.1).

`move-line-up` and `move-line-down` take `Alt+Up` and `Alt+Down`, which the editing area also reads
itself (§3). That is the same action by both routes, so it is not a conflict, and the editing area's
reading is taken out of the frame before the menu watcher sees it, so the line does not move twice.

## 5. How a chord is chosen for an action

`services::keymap` is all of it, pure and tested with no window.

### 5.1 One chord, written one way

A chord is a `Shortcut` (`app::actions`). It is written as the menus already show it: modifiers then the
key, joined with `+`, the key spelled as `actions::key_name` spells it (`Ctrl+Shift+/`, `Ctrl+,`,
`Ctrl+Plus`). `keymap::parse` reads that form and also the names people type: `Cmd`, `Command`, `Ctrl`,
`Control`, `Alt`, `Option`, `Opt`, `Shift`, any case, `-` or `+` between parts, the key by egui's own
name or its character (`Slash` or `/`, `Comma` or `,`).

What `Cmd` and `Ctrl` mean depends on the platform, because `Shortcut::matches` already does:

- On Windows and Linux there is one control key. `Ctrl`, `Cmd` and `Command` all read as `command`, which
  is how every chord in Unluminous is held, so a settings file written on a Mac means the same on Windows.
- On macOS `Cmd` is `command` and `Ctrl` is `ctrl`, the two separate keys.

Two chords are **the same chord** when they would match the same key press. `keymap::same` is that
comparison, made by comparing `Shortcut::normalised`, which folds `ctrl` into `command` off macOS, `Equals`
into `Plus`, and the shifted spellings `Questionmark` and `Pipe` into `Slash` and `Backslash`.

### 5.2 The order things are decided in

For every bindable action, its chord is the first of these that says anything:

1. the person's own binding for it (§6), which may be a chord or **none**;
2. the preset's row for it, on this platform;
3. its Unluminous chord, from the menu entry.

Then conflicts are settled, highest first: a chord given by step 1 takes the chord away from any action
that got it from step 2 or 3, and a chord given by step 2 takes it away from any action that got it from
step 3. Two of the person's own bindings cannot share a chord, because binding one takes it from the
other at the moment it is bound (§7.3), and a settings file edited by hand to give two actions one chord
keeps the one whose name sorts first and reports the other in `settings keymap`.

The answer is a `keymap::Resolved`: the chord of every action whose chord is **not** its Unluminous one,
including the ones that now have none. It is worked out when the settings change, never once a frame, and
it is held by the window in an `Arc` so the menu state copies a pointer.

### 5.3 Applying it to the menus

`actions::menus` builds the bar as it always has and then calls `keymap::apply`, which walks every entry and
replaces the shortcut of each one whose action's name is in the `Resolved` map. The context menus that show
a chord (`tab_menu`, `text_menu`, `gutter_menu`, `folding_menu`, `explorer_menu`, the realm menus) get the
same walk, so a row shows the same chord wherever it is drawn. Because the menu watcher, the macOS menu bar,
`action list`, the command palette and the title bar all read the menus, all of them follow the keymap with
no change of their own.

`SECOND_CHORDS` (`Ctrl+Alt+Left` and `Right` for going back and forward) stays, but it is checked after
every menu entry, so a chord the keymap gives to another action wins over it.

### 5.4 The menu watcher takes the key press it acted on

Before this ticket the menu watcher read a chord and left the key press in the frame, and every default
chord was chosen so that nothing else read the same key. A preset cannot keep that promise: IntelliJ's
`Replace` is `Ctrl+R`, and in a Markdown file the editing area reads `Ctrl+R` itself as align right, so
one press would have opened Replace and aligned the paragraph. So `read_the_menu_shortcuts` now takes the
press out of the frame once an entry acts on it. Two exceptions keep what was there: a chord the focused
text box owns (undo, redo, select all) is left for the text box, and while a terminal has the keyboard
the press is left in, because a program in a terminal has always been sent the chord as well.

### 5.5 Moving a line is on the menu now

`Alt+Up` and `Alt+Down` moved a line with no menu entry, so they could not be listed or changed.
`Code -> Move Line Up` and `Move Line Down` are entries now, marked as not read by the menu watcher.
`code_editing::route_the_line_move_keys` still reads them, because it reads them only while the editing
area has the keyboard, and it now asks the keymap which chord they are on. On macOS they are shown in the
menu but are not key equivalents, or AppKit would move a line while a terminal had the keyboard.

### 5.6 The one place the reference editor is named

`naming::tests::no_shipped_text_names_the_other_tool` (`task-1795`) refuses the name of the editor
Unluminous was measured against anywhere that ships. A preset a person picks has to be called by its
name, so the keymap's own files, and any other line that is about the keymap, are exempt. Every other
line is still held to the rule.

## 6. Where it is written

Two kinds of line in `settings.conf`, read and written by the `settings!` macro like every other setting:

```
keymap.preset = intellij
keymap.key.duplicate-line = Ctrl+Shift+D
keymap.key.git-show-diff = none
```

`keymap.preset` is a coded setting (`unluminous`, `vscode`, `intellij`), so `settings get`, `settings set`,
`settings list` and `settings reset` reach it with no new code. The bindings are an `extra` field,
`Settings::key_bindings`, a list of action name and chord text. The text is kept as written rather than
parsed into a `Shortcut` on the way in, so a file written on a Mac that names `Cmd` reads the same on a
Windows machine, and a line naming an action this build has not got is kept and written back rather than
dropped, which is the rule `settings.conf` already keeps for values it does not recognise. Every
`keymap.key.` line is taken out of the file before the list is written, so a binding that was reset does
not come back.

## 7. The Settings page

`Settings -> Keymap`, its own page between `Theme` and `Editor`, where IntelliJ puts it. Drawn with
`settings_kit` like every other page.

### 7.1 Preset

A card with one row: `Preset`, a segmented choice of `Unluminous`, `VS Code` and `IntelliJ`, and under it
how many of the person's own changes are on top (`3 of your own changes are kept on top of the preset`)
with `Reset all` beside it when there are any. Choosing a preset takes effect at once, like every setting.

### 7.2 Shortcuts

A card holding a search well and the list. Each row is the action's menu path in the quiet ink and its
name, then the chord as a chip on the right, then `Reset` when the row has the person's own binding. A row
the preset changed says `VS Code` or `IntelliJ` in small words beside its chip, so it is clear where a
chord came from, which is VS Code's `Source` column.

The search matches the name, the menu path and the chord text, ignoring case, spaces and whether `+` or `-`
joins the parts. Rows are drawn only when they are inside the visible part of the page, so a list of about
a hundred and forty rows costs the rows on the screen.

The rows that cannot be changed (Cut, Copy, Paste) are listed at the end of the list with the reason, so
nobody searches for `Copy` and finds nothing.

### 7.3 Recording a chord

Pressing a row's chip makes that row the one recording: the chip says `Press a shortcut` in the accent, and
the next key press with its modifiers is read from the frame's events. While a row is recording:

- **The menu watcher stands aside** (`read_the_menu_shortcuts` asks `settings_window.keymap.recording`), or
  pressing `Ctrl+S` to bind it would also save. On macOS the bar's key equivalents are taken away while
  recording, for the same reason, because AppKit hands a key equivalent to the menu before the window.
- A bare modifier press is ignored, so holding `Ctrl` and then pressing `D` records `Ctrl+D`.
- `Escape` stops recording and changes nothing. `Backspace` or `Delete` with no modifier records **none**.
- A key with no modifier is refused unless it is a function key, because a bare letter would fire while
  typing. The refusal is written under the list.
- A chord in the reserved list (§8) is refused with the name of what owns it.
- A chord another action has is **taken from it**: the other action gets a binding of none, and the line
  under the list says `Ctrl+D was taken from Git › Show Diff.` This is IntelliJ's "remove other
  assignments", made the only answer, because a chord on two entries is a chord one of them can never use.
- egui turns `Ctrl+C`, `Ctrl+X` and `Ctrl+V` into clipboard events before a key event exists, so those
  three can never be recorded, and they are in the reserved list anyway.

A click anywhere else, or closing the dialog, stops recording.

## 8. Reserved chords

Chords the recorder and `settings bind` refuse, because something other than a menu entry reads them:

| Chord | What owns it |
|---|---|
| `Ctrl+C`, `Ctrl+X`, `Ctrl+V` | the clipboard, delivered by egui as events |
| `Tab`, `Shift+Tab`, `Enter`, `Escape`, `Backspace`, `Delete`, the arrows, `Home`, `End`, `PageUp`, `PageDown` with no modifier | typing and moving the caret |
| `Shift` with any of those | extending a selection |
| `Ctrl+Left`, `Ctrl+Right`, `Ctrl+Backspace`, `Ctrl+Delete`, `Ctrl+Home`, `Ctrl+End`, with or without `Shift` | moving and deleting a word, and the start and end of the file |
| `Ctrl+Shift+L`, `Ctrl+Shift+E`, `Ctrl+Shift+R`, `Ctrl+Shift+J` on a Markdown file | the paragraph alignment the editing area reads itself |

The last row is not refused, only warned about, because the alignment chords only act in a file that has
formatting; IntelliJ's `Join Lines` on `Ctrl+Shift+J` is the one preset row it touches, and in a source
file it is Join Lines.

## 9. The agent's half

Everything a person can do on the page an agent can do through `unluminous-cli`:

- `settings set keymap.preset intellij` chooses a preset, through the existing command.
- `settings keymap [filter] [--changed]` lists every bindable action: its name, its menu path, its chord
  now, the preset's chord and the Unluminous chord, and where the chord came from (`unluminous`, `preset`
  or `yours`). `--changed` lists only the ones that differ from Unluminous's own.
- `settings bind <action> <chord|none>` binds a chord, with the same refusals and the same taking from
  another action as recording does, and answers which action lost the chord. `settings bind <action>
  --reset` takes the person's own binding away, and `settings bind --reset-all` takes them all away.

The MCP tools come from the catalogue, so `unluminous_settings` offers both verbs with no other change.
`unluminous-cli/docs/commands.md` gets a section for each.

## 10. Tests

- `services::keymap` unit tests: parsing every spelling, platform meaning of `Cmd` and `Ctrl`, `same` across
  `Plus` and `Equals`, the order of §5.2, a preset taking a chord from an Unluminous default, a person's
  binding taking it from a preset, `none` removing a chord, an unknown action kept and written back.
- **Every preset row names a bindable action and parses on both platforms**, and **no two actions share a
  chord in any preset's resolved keymap on either platform** except the `SHARED_BY_STATE` pair. That is
  `no_two_menu_items_claim_one_key_equivalent`, the test the menus already have, run once per preset.
- `actions` tests: with the IntelliJ preset, `Ctrl+D` is `duplicate-line` and `Show Diff` shows no chord;
  with VS Code, `F12` goes to the definition; a person's binding moves the chord in the menu bar and the
  menu watcher follows it.
- The settings round trip: `keymap.preset` and `keymap.key.*` written and read back, a reset binding not
  coming back.
- Screenshot tests: the Keymap page with the Unluminous preset, with IntelliJ and a changed row, and a row
  recording. A window test that records a chord by sending its key press and then fires the action with it.
- The command line: `settings bind` and `settings keymap` driven through `run_cli`, and the documentation
  test that every command has its section.

Then the real window: install the build, choose IntelliJ on the page, press `Ctrl+D` in a file and see the
line duplicated, bind `Ctrl+Shift+D` to `Show Diff` from the command line, and read the menu.
