# task-2336: Unluminous issues v28

Five reports, one ticket. Each section says what was reported, what the code does today, what changes,
how an agent reaches the same thing, and how it is tested.

| # | Report | Where the change is |
|---|---|---|
| 1 | `Cmd`+`+`/`-` and `Cmd`+wheel over a browser node should zoom the page | `app/zooming.rs`, `app/realm.rs`, `services/browser.rs`, `services/browser_zoom.js` |
| 2 | New file entries for common types on the explorer's right click menu, and IntelliJ style templates | `app/actions.rs`, `services/file_templates.rs`, `app/menus.rs`, `app/modals.rs` |
| 3 | A new tab from a right click in the editor, a Save dialog with a folder tree, and the same dialog on close | `app/actions.rs`, `components/save_dialog.rs`, `app/opening.rs`, `app/files.rs` |
| 4 | Text in the ticket modal's fields shifts while typing | measured first, then fixed where the measurement points |
| 5 | Tool calls in the agent chat show raw JSON | `components/agent_chat/tool_view.rs` |

## 1. Zooming a browser node's page

### What happens today

A browser node draws its page in a native child window. The page's zoom is the product of the node's
own zoom (`Live::page_zoom_of`) and the camera's, sent on the node's `BrowserPlacement` and applied by
`native::place` only when the product changes.

- **The keys while the canvas has the keyboard.** `Action::ChangeFontSize` goes to
  `the_pane_the_keys_zoom()`, which answers `Panel::Realm`, and `step_the_zoom_of(Panel::Realm)`
  zooms the **camera**. Hovering a browser node changes nothing.
- **The keys and the wheel while the page has the keyboard.** On Windows the view is built with
  `with_hotkeys_zoom(true)`, so WebView2 zooms the page itself. Nothing tells Unluminous, so
  `page_zoom_of` still says 1.0 and `NativeView::zoom` still holds the number Unluminous last sent. The
  next change of camera zoom sends `page_zoom_of × camera` and the person's zoom is thrown away. On
  macOS `with_hotkeys_zoom` is unsupported by `wry`, so `Cmd`+wheel over the page does nothing at all,
  and `Cmd`+`=` reaches the menu bar, which zooms the camera.
- **The wheel over the node's toolbar or frame** already works: `zoom_over_a_node` reads the
  modifier wheel and calls `zoom_a_node`, which for a browser node steps `page_zoom_of`.

### What changes

1. **The zoom keys go to the browser node under the pointer.** When the keys are the canvas's,
   `step_the_zoom_of(Panel::Realm)` and `reset_the_zoom_of(Panel::Realm)` first ask
   `browser_node_the_zoom_keys_are_for()`. That answers the browser node under the pointer, asking
   the operating system for the pointer (`system_files::pointer`) because egui is told nothing while
   the pointer is over the native page. When the pointer is over no node, it answers the chosen node if
   that is a browser node whose page holds the keyboard. With an answer, the keys call
   `zoom_a_node(node, steps)` or put `page_zoom_of` back to 1.0. With none, the camera zooms as before.
2. **The page reports the gesture instead of acting on it.** An initialization script,
   `services/browser_zoom.js`, is put in every page Unluminous shows. It listens for `wheel` with
   `ctrlKey` or `metaKey`, and for `keydown` of `=`, `+`, `-` and `0` with `Ctrl` (not on macOS, where
   the menu bar takes `Cmd`+`=` before the page sees it). It listens in the bubbling phase and does
   nothing when `event.defaultPrevented` is already set, so a page that handles its own modifier wheel,
   such as a map, keeps it. Otherwise it calls `preventDefault`, which stops WebView2's own zoom, and
   posts `unluminous-zoom:in`, `unluminous-zoom:out` or `unluminous-zoom:reset` through
   `window.ipc.postMessage`. The wheel is added up and one step is posted per 100 units of `deltaY`,
   which is one notch of a mouse wheel, so a trackpad does not step on every event.
3. **The host turns the message into `BrowserEvent::ZoomAsked { id, steps }`** through `wry`'s
   `with_ipc_handler`. The handler reads only those three strings and ignores anything else, so a page
   can ask its own view to zoom and nothing more. `steps` is `0` for a reset.
4. **`act_on_browser_events` routes it by who owns the tab.** A tab on a node calls
   `zoom_a_node(node, steps)`, or sets `page_zoom_of` back to 1.0 for a reset. A tab in a pane calls
   `step_the_page_zoom` or `reset_the_page_zoom`, so `BrowserTab::zoom` is true for panes as well,
   where it was also out of date after an engine zoom.

`with_hotkeys_zoom(true)` stays, so pinch zoom on a touch pad still works where the script does not
reach.

### The agent's half

`realm zoom <node> --bigger | --smaller | --reset` already does what the gesture does, through
`zoom_a_node`. Nothing new is needed.

### Tests

- `act_on_browser_events` fed a `ZoomAsked` for a node's tab steps `page_zoom_of`, and for a pane's tab
  steps `BrowserTab::zoom`. A reset puts each back to 1.0.
- `browser_zoom_message` parses the three strings and refuses everything else.
- A screenshot test is not useful here: the native page is not in egui's picture.

## 2. New file entries and file templates

### What IntelliJ does

IntelliJ's templates are under `Settings | Editor | File and Code Templates`. Each has a name, an
extension, a body, and in later versions a file name pattern. The body is Velocity: `${NAME}` and the
other predefined variables (`${DATE}`, `${TIME}`, `${YEAR}`, `${MONTH}`, `${DAY}`, `${HOUR}`,
`${MINUTE}`, `${SECOND}`, `${USER}`, `${PROJECT_NAME}`, `${DIR_PATH}`, `${FILE_NAME}`, `${DS}` for a
literal dollar sign) are filled in when the file is made. A variable that is not predefined and not set
with `#set` is asked for in the New dialog. `#parse("File Header.java")` includes another template
from the Includes tab. Templates have two scopes: **Default**, kept with the IDE's settings, and
**Project**, kept in `.idea/fileTemplates` as `<name>.<extension>.ft` files. `Tools | Save File as
Template` makes a template from an open file. Every template shows up under `File | New` and the
project view's `New` menu.

### What Unluminous does

A template is a file named `<Name>.<extension>.ft`, the same shape IntelliJ keeps, in one of two
folders:

| Scope | Folder | Wins when two share a name |
|---|---|---|
| Project | `<project>/.unluminous/templates/` | yes |
| User | `<settings folder>/templates/` (`%APPDATA%\Unluminous\templates` on Windows) | no |

Includes are files in an `includes` folder beside them, named `<Name>.<extension>.ft` too.

`services/file_templates.rs` holds it all, with no window:

- `Template { name, extension, body, scope }`, and `templates(project, user) -> Vec<Template>`, sorted by
  name, the project's first.
- `built_in()`: the five common types the ticket names, which are templates too, so they share the
  one path: **Rust File** (`.rs`), **Text File** (`.txt`), **JavaScript File** (`.js`), **HTML File**
  (`.html`, a small HTML 5 page whose title is `${NAME}`), and **CSS File** (`.css`). Each body is short.
  A project or user template with the same name replaces the built in one.
- `expand(body, values, includes) -> Result<String, String>`: `${NAME}`, the predefined variables
  above, `$NAME` without braces, `${DS}`, `#set( $NAME = "value" )` and `#parse("Name.ext")` (one level
  deep, refused past eight to stop a loop). Unknown variables are listed by `variables_asked(body)`.
- `save_as_template(path, name, folder)`: `Save File as Template`.

### The menu

The explorer's `New` submenu becomes:

```
New
  File
  Folder
  ---
  Rust File
  Text File
  JavaScript File
  HTML File
  CSS File
  Jupyter Notebook
  ---
  Templates  >  <every project and user template>
                ---
                Save File as Template...      (dimmed unless a file was right clicked)
                Open Templates Folder
```

Each typed entry is `Action::NewFileFromTemplate { folder, template }`. It opens the existing New File
prompt with the name selected and the extension already typed (`untitled.rs` with `untitled`
selected), so a person types the name once. When the template asks for variables of its own, one
prompt per variable follows, titled with the variable's name. The file is written, the tree reloaded
and the file opened, the way New File already does. `Open Templates Folder` makes the user folder if
it is missing and shows it in the system file manager. `Save File as Template...` asks for a name and
writes `<name>.<extension>.ft` into the user folder.

### The agent's half

- `explorer new-file <path> --template <name> [--set KEY=VALUE]...` writes the expanded template.
  A variable that is asked for and not given with `--set` is refused, naming the variable.
- `explorer templates` lists every template with its scope, extension and the variables it asks for.
- `explorer save-template <path> --name <name> [--project]` is `Save File as Template`.
- The menu entries are reachable through `action run` because they are menu entries.

### Tests

- `file_templates` tests: expansion of each predefined variable, `#set`, `#parse`, the include loop
  refusal, `${DS}`, a project template replacing a user one and a built in one, the file name parse of
  `A Name.tsx.ft`, and `variables_asked`.
- `explorer_menu` test: the New submenu holds the five types and the Templates submenu.
- A CLI test: `explorer new-file x/page.html --template "HTML File"` writes the page with `page` as its
  title; `explorer templates` lists a template dropped into `.unluminous/templates`.

## 3. A new tab from the editor, and a Save dialog with a folder tree

### What happens today

The editor's right click menu (`actions::text_menu`) holds Cut, Copy, Paste, Select All and the symbol
entries. An untitled tab exists (a fresh window has one), but `save` writes it to `untitled.md` in the
project root without asking, and `save_before_closing` closes it without saving and says so.

### What changes

1. **The editor's right click menu gets a `New Tab` submenu**: `Untitled...`, the five types, and the
   templates, the same list section 2 builds. Each is `Action::NewUntitledTab { template }`. It asks for
   the name with the type's extension already typed, then opens a tab holding the expanded template,
   with **no path**. The name is kept on the tab as `OpenFile::proposed` and the tab shows it, so the
   tab reads `notes.rs` and is coloured as Rust. `OpenFile::language_path()` answers the path, or the
   proposed name when there is no path, and colouring and `grammar_for` ask it.
2. **Save on a tab with no path opens the Save dialog**, `components/save_dialog.rs`. It is a modal in
   the style guide's shape with:
   - a folder tree of the project, folders only, drawn the way the explorer draws its rows (the same
     icons, indent and selection pill), opened at the project root, with the chosen folder marked;
   - a `Name` field holding the proposed name, with the stem selected;
   - the full path it will write, under the field;
   - `Cancel` and `Save`. `Enter` saves, `Escape` cancels.
   A name that already exists in the chosen folder is refused in the dialog, with the reason under the
   field. Saving writes the file, gives the tab its path, reloads the tree and reveals the file.
3. **Closing a modified tab with no path opens the same dialog** with a third button, `Don't Save`,
   which closes the tab and throws the text away. `Cancel` keeps the tab open. A tab with no path and no
   changes closes without a question, as now. `File -> Save` with no path no longer writes
   `untitled.md` anywhere.

The dialog's state lives on the window as `SaveDialog { tab, folder, name, closing, expanded }`.
`tab` is the tab's identity rather than its index, because the dialog is answered on a later frame.

### The agent's half

- `tab new [--name <name>] [--template <name>] [--set KEY=VALUE]...` opens the untitled tab.
- `tab save` on a tab with no path opens the dialog and says so; `tab save-as <path>` writes it with no
  dialog, as it already does for any tab.
- `modal state` reports the Save dialog's folder, name, target and whether Save is refused. `modal type
  <name> --folder <folder>` fills it, `modal results` lists the folder rows and `modal choose <row>` picks
  one, `modal accept` saves, `modal accept --discard` is `Don't Save`, and `modal cancel` keeps the tab.

### Tests

- `files` tests: a tab with a proposed name answers it as its name and its language path.
- App tests: `NewUntitledTab` for Rust opens a tab named `x.rs` with no path; `save` on it opens the
  dialog; answering the dialog writes the file into the chosen folder and the tab gets that path; the
  dialog refuses an existing name; closing a modified untitled tab opens the dialog with `Don't Save`,
  and `Don't Save` closes it without writing.
- A screenshot test of the dialog, as every modal has.

## 4. Text that shifts while typing in the ticket modal

### What is known

The ticket modal is drawn with `rux` at a fixed size (`Look::at_a_fixed_size`) and its fields are
`rux::components::TextInput` and `TextArea`, which are `egui::TextEdit` inside. A modal can be zoomed
(`task-2198`), and `crisp::sharpen_the_text_in` lays every galley out again at the composited size once
the layers are drawn. The report does not say which fields or at what zoom.

### How it is found

`tools/text-shift/measure.mjs` drives a real window (`tools/drive-a-window.ps1`, a scratch `APPDATA`):
it opens a ticket, and for each field (title, description, todo, comment) and each modal zoom of 0.8,
0.9, 1.0, 1.1, 1.25 and 1.5 it types a sentence one character at a time, takes `window screenshot`
after each character, and compares the pixels of the text typed before that character with the
previous picture. Any column of the earlier text that moved is a shift, reported by field, zoom,
character and distance in pixels. The numbers go in `_agent_output/task-2336-text-shift/`.

### What changes

Whatever the measurement shows is fixed at its cause and the same run is repeated to show zero shifted
columns at every zoom.

### What was measured, and the cause

The measurement became a window test, `typing_in_a_ticket_never_moves_the_text_already_typed` in
`tests/plugins_board_and_chat.rs`, rather than a script against a live window: it opens a ticket in the test
harness, types `The quick brown fox jumps; Wavy AV pull 0.5 tij` into the title and the comment box one
character at a time, renders after each character, and finds the leftmost column that changed. Anything left
of where the previous character began is earlier text moving.

| Display density | Modal zoom | Title: keystrokes that moved earlier text | Comment |
|---|---|---|---|
| 1.0, 1.25, 1.5, 2.0 | 1.0 | 0 | 0 |
| 1.0 | 1.1 | 8 of 47, by 11 to 114 px of columns | 0 |
| 1.0 | 1.25 | 6 of 47, by 11 to 160 px of columns | 0 |
| 1.5 | 0.9 | 0 | 0 |

So it is the zoomed modal and nothing else. A zoomed modal's text is laid out again at the composited size
by `theme::crisp::sharpen`, and `keep_the_original_places` then stretched each sharp row by one ratio: the
original row's width over the sharp row's. That ratio depends on every letter in the row, so typing a letter
at the end changed it and moved every letter before it. The title moves most because it is 20 points with
negative tracking, where each letter's rounding is largest; the comment box at 13 points moved by less than
a pixel.

### The fix

Each letter is now corrected by a running average of the drift between its sharp place and the caret's
place, followed from the left only (`FOLLOW_THE_DRIFT = 0.35`). A letter's place depends on the letters
before it and never on the ones after it. The two earlier guarantees still hold and are still tested: the
ink stays within a pixel of where the caret was measured (`task-2270`) and two neighbours keep the font's own
gap to within a pixel (`task-2321`). After the fix the same run measured 0 of 658 keystrokes moving earlier
text, and `a_letter_added_at_the_end_leaves_the_sharp_letters_before_it_where_they_were` checks the layout
directly at 13 and 20 points and three zooms.

## 5. Tool calls in the agent chat

### What happens today

An open tool call shows its arguments as one pretty printed JSON fence and its answer as a second
fence. Keys, quotes and braces fill the block, a shell command is a JSON string with `\n` escapes, and
an edit's old and new text are two long strings.

### What changes

`components/agent_chat/tool_view.rs` turns a call into Markdown that the chat already renders, as a
pure function `tool_markdown(tool) -> String`:

- **The command first.** A `command` or `cmd` argument is a `sh` fence with the command as it would be
  typed. A `description` argument is a quiet line above it.
- **An edit is a diff.** `old_string` and `new_string` (Claude Code's `Edit`, and the same pair in other
  agents) become one `diff` fence, `-` lines then `+` lines. `MultiEdit`'s `edits` list becomes one diff
  per edit.
- **Written content is in its language.** `content` with a `file_path` is fenced with the file's
  extension, so the JSON plugin colours JSON and the Rust plugin colours Rust.
- **Everything else is a list of short values.** Strings of one line, numbers and booleans are lines of
  a list, the name in words and in bold (`file_path` is `File path`) and the value as inline code. A Markdown
  table was tried first and drew a ruled grid with an empty header row, which was heavier than two words.
  Long strings and nested objects or lists get a labelled fence of their own, JSON for structures.
- **The answer is labelled `Result`** and fenced in the language of the file the call read when it read
  one, as JSON when it is JSON, and as plain text otherwise. An answer longer than 400 lines shows its
  first 400 and says how many were left out.
- Arguments that are not JSON are shown exactly as they arrived, as today.

### The agent's half

Display only. `plugins run agent-chat` reads the conversation as data and is unchanged.

### Tests

`tool_view` tests over real argument shapes: Bash, Read, Edit, MultiEdit, Write, Grep, an MCP call with
a nested object, a non JSON argument, a long answer cut at 400 lines. A screenshot test of a chat with
an open Bash call and an open Edit call.

## Order of work

5, 1, 2, 3, 4. Each is built, installed and launched before the next, as `CLAUDE.md` asks, and the
whole is released once at the end with `tools/release.ps1 -Part minor`.
