# Settings inventory

Every setting the Settings modal shows today, read out of the code at `task-2244`. It is an inventory for
the designer: nothing here is a proposal.

Where the window keeps its settings:

- **The window's own settings** are `name = value` lines in the person's settings file, read and written by
  `crates/unluminous-app/src/settings.rs`. The key column below is the name in that file. The same name works
  with `unluminous-cli settings set <key> <value>`.
- **A plugin's settings** are in that plugin's own `settings.conf` (Agent-Chat, Agent-Tasks) or
  `sources.conf` (Database), in the plugin's folder.
- **Every change takes effect at once.** There is no Apply. The footer has one button, `Done`, and the line
  "Changes take effect at once.".

Files that draw the pages:

| Page | File |
|---|---|
| The modal, the nav, Appearance, Theme, Editor, Terminal | `crates/unluminous-app/src/components/settings_dialog.rs` |
| MCP | `crates/unluminous-app/src/components/mcp_page.rs` |
| Plugins | `crates/unluminous-app/src/components/plugins_page.rs` |
| Agent-Chat | `crates/unluminous-app/src/components/agent_chat/settings_page.rs` |
| Agent-Tasks | `crates/unluminous-app/src/components/agent_tasks/settings_page.rs` |
| Database | `crates/unluminous-app/src/components/database/settings_page.rs` |

## The modal itself

- Size: 900 by 680 points. Header 46, footer 52, nav column 258 wide.
- Header: `Settings — <project folder name>` (or `Settings` with no project), and a round cross named
  `Close settings`.
- Nav: a `Search settings` field at the top, then the pages under their group headings. A search with no match
  shows `No setting matches`. Search matches a page's title and group (for Unluminous's own pages, also the
  section names listed in `Page::sections`).
- Right side: a breadcrumb (`Group › Page`, or the page name alone when it has no group), then the page. A page
  taller than the body scrolls with the thin document scrollbar.
- Footer: `Changes take effect at once.` on the left, `Done` on the right. Enter presses `Done`.

**The nav order in the code** (`Page::all` in `settings.rs`, groups from `Page::group`):

1. Appearance & Behavior › Appearance
2. Appearance & Behavior › Theme
3. Editor › Editor
4. **Plugins** (no group heading, drawn between Editor and Tools)
5. Tools › Terminal
6. Tools › MCP
7. Plugins › Agent-Chat, Agent-Tasks, Database (one row per plugin whose manifest has `settings.page`, in the
   order the plugins are listed)

The ticket's order leaves out item 4. It is included below, after MCP.

---

## Appearance & Behavior › Appearance

### Font

| Label | Control | Options / range | Default | Key |
|---|---|---|---|---|
| Family: | Dropdown, named `Editor font family` | Every font family on this machine. Shows `System default` when empty | empty (system default) | `appearance.font.family` |
| Size: | Dropdown, named `Editor font size` | 9, 11, 13, 16, 20, 24, 32, 48, 64. The file accepts 6 to 144 | 16 | `appearance.font.size` |

Help text (two lines): "The font the editor sets every open file in. Bold, italic and colour stay as they
were," / "and the size is also on the keyboard at command or control with plus and minus."

### Interface

| Label | Control | Options / range | Default | Key |
|---|---|---|---|---|
| Family: | Dropdown, named `Interface font family` | `The editor's`, then every family on this machine | empty, shown as `The editor's` | `appearance.ui.font.family` |
| Size: | Dropdown, named `Interface font size` | 10.0, 11.0, 12.5, 14.0, 16.0, 18.0, 20.0. The file accepts 8 to 24 | 12.5 | `appearance.ui.font.size` |

Help text: "The menus, the rail, the explorer and the status bar. The editing area keeps its own font above."

### Background

| Label | Control | Options / range | Default | Key |
|---|---|---|---|---|
| Opacity: | Slider (220 wide) with the percentage written beside it, named `Background opacity` | 5% to 100% | 86% | `appearance.background.opacity` (0.05 to 1.0) |
| Behind: | Button showing the current choice, named `Background`. Opens the background picture grid | `Show Contents Underneath` (empty) or a picture's file name, cut to 34 characters | `forest-1.jpg` on a fresh install (chosen when no settings file exists yet); empty in the settings model | `appearance.background.image` |

Help text under Opacity: "Fades the window so what is behind shows through. Text stays fully solid at every
setting."

Help text under Behind: "A picture behind the window, or the desktop. Pictures chosen here are kept in
Unluminous's own folder, so moving the original does not lose it."

The picture grid that `Behind:` opens is a separate modal and is not part of this page.

### Plugin panes

| Label | Control | Default | Key |
|---|---|---|---|
| Draw depth in plugin panes | Tick box | on | `plugins.chrome` |

Help text: "Soft shadows, gradients and pressed edges behind a plugin's own pane, drawn on the processor. Off, a
plugin draws flat, which costs nothing at all."

---

## Appearance & Behavior › Theme

### Theme

| Label | Control | Options | Default | Key |
|---|---|---|---|---|
| (no label) | A list of rows, one per installed theme. Each row: the theme's name, the plugin it came from in faint text, and six colour swatches on the right (editor, explorer, accent, git added, unsaved, close). The chosen row is drawn as a filled pill | Unluminous Dark, Unluminous Light, and every theme a theme plugin carries (the bundled `themes-bundle-1` has Islands Dracula Colorful, Material Palenight, Material Deep Ocean, Monokai Pro, One Dark) | Unluminous Dark, stored as empty | `appearance.theme` (`<plugin>/<theme>`, empty is Unluminous Dark) |

Help text: "A theme says what every colour in Unluminous's own palette means. One that names the nine token
colours also recolours code, in every language at once, in the same frame."

### Accent

| Label | Control | Options | Default | Key |
|---|---|---|---|---|
| Accent: | Eight round swatch buttons in a row, 24 points each. Each has a tooltip with its name. The chosen one has a ring | `The theme's own` (drawn hollow), `Accent`, `Unsaved`, `Added`, `Modified`, `Agent`, `Blame`, `Close`. The colours are the active theme's own, so they change with the theme | `The theme's own` (empty) | `appearance.accent` (`#RRGGBB`, empty for the theme's own) |

Help text: "One colour for everything the accent means: the caret, the open tab, an open folder and the line a
program is stopped on. The first is whatever the theme chose."

### Icons

| Label | Control | Options | Default | Key |
|---|---|---|---|---|
| Icon set: | Dropdown, named `Icon set` | `Follow the theme`, `Classic`, `Material` | `Follow the theme` (empty) | `appearance.icons` (`material`, `classic` or empty) |

Help text: "Which drawn marks the rail buttons, the folder arrow and the small controls use. Material draws a
chevron where the classic set draws a triangle, and puts a folder in front of a folder's name."

---

## Editor › Editor

### Gutter

| Label | Control | Default | Key |
|---|---|---|---|
| Show line numbers | Tick box | on | `editor.line_numbers` |

Help text: "A number against each line of the file. Unluminous wraps, so a paragraph that runs over several rows
is numbered once, against its first row. Right clicking the gutter puts the numbers away and annotates with git
blame."

### Notebooks

| Label | Control | Default | Key |
|---|---|---|---|
| Scroll long outputs | Tick box | on | `notebook.scroll_outputs` |

Help text: "An output taller than thirty lines scrolls inside its cell. Off, every output is drawn at its full
height."

### Suggestions

| Label | Control | Values | Default | Key |
|---|---|---|---|---|
| Suggest completions as you type | Tick box. Ticked is `automatic`, unticked is `manual` | `automatic`, `manual` | ticked (`automatic`) | `editor.suggestions` |
| Ask language servers (rust-analyzer, tsserver) | Tick box. Ticked is `automatic`, unticked is `off` | `automatic`, `off` | ticked (`automatic`) | `editor.servers` |

Help text under Suggest completions: "A list of names appears under the caret once two letters of a word have
been typed, in a file whose language a plugin claims. Off, nothing appears until you ask: Ctrl+Space, or Complete
Word on the Edit menu, which work either way."

Help text under Ask language servers: "Rust and TypeScript files ask the language server the project already has
for the members of a value, the import a name needs and a call's parameters. Off, no server is started and the
structural answers stand alone."

### Indentation

| Label | Control | Options | Default | Key |
|---|---|---|---|---|
| One indent is: | Dropdown, named `One indent is` | `Tabs`, `2 spaces` … `8 spaces` | `Tabs` | `editor.indent` (`tabs` or `spaces:N`, N from 2 to 8) |
| Indent a new line like the one above it | Tick box | | on | `editor.auto_indent` |
| Trim trailing whitespace on save | Tick box | | off | `editor.trim` |

Help text under One indent is: "What the Tab key types where nothing is selected. Indenting a selection still
moves each line by one character, which is what Tab over a selection has always done."

Help text under Indent a new line: "Pressing Enter starts the new line with the whitespace the line it was
started from begins with."

Help text under Trim trailing whitespace: "Off by default, and never on a Markdown file, where two spaces at the
end of a line are a line break."

### Debugger

| Label | Control | Values | Default | Key |
|---|---|---|---|---|
| Show value tooltip | Tick box. Ticked is `automatic`, unticked is `manual` | `automatic`, `manual` | ticked | `debug.value_tooltip` |
| lldb path: | Text field, 300 wide, named `lldb adapter path` | a path, or empty | empty | `debug.lldb` |
| node path: | Text field, 300 wide, named `node adapter path` | a path, or empty | empty | `debug.node` |

The two path rows come from the list of debuggers (`DEBUGGERS` in `services/plugins/registries.rs`), so a third
debugger would add a third row.

Help text under Show value tooltip: "While a program is stopped, resting the pointer on a name shows what it
holds, and a structure opens into its fields, which can be typed over. Off, nothing appears until you ask: Show
Value on the Debug menu."

Help text under the two paths: "Leave empty to search the usual places; the debug tile offers to install one
when none is found."

### Files

| Label | Control | Options | Default | Key |
|---|---|---|---|---|
| Line endings: | Dropdown, named `Line endings` | `Keep what the file has`, `Always LF (Unix)`, `Always CRLF (Windows)` | `Keep what the file has` | `editor.line_ending` (`keep`, `lf`, `crlf`) |
| Exclude: | Text field, 300 wide, named `Exclude` | comma separated `.gitignore` patterns | empty | `editor.exclude` |

Help text under Line endings: "What a file is written back with. Kept as it was found is what leaves a one
character edit as a one line diff; the other two bring every file that is saved into line."

Help text under Exclude (two notes): "Patterns Go to File, Find in Files, completion and Go to Definition leave
out, separated by commas, written the way a .gitignore line is." / "The project's own .gitignore is read
already; this is a list beside it. The explorer goes on showing everything either way."

### Updates

| Label | Control | Values | Default | Key |
|---|---|---|---|---|
| Check for a newer version once a day | Tick box. Ticked is `daily`, unticked is `off` | `off`, `start`, `daily` | ticked (`daily`) | `update.check` |

Help text: "One request to unluminous.com a day, shared by every window. A newer version is offered in a notice
with Install & Restart and Don't Ask Again. Off, Unluminous sends nothing until you ask: Check for Updates on the
Unluminous menu works either way."

### Not built yet / not on this page

- `update.check = start` (ask every time a window opens) exists in the settings model
  (`settings.rs`, `UpdateCheck`) but the tick box can only choose `daily` or `off`. It can be set with
  `unluminous-cli settings set update.check start`.
- `update.skip` (the version `Don't Ask Again` was pressed on) is a setting with no control on this page. It is
  set from the update notice (`settings.rs`, field `update_skip`).
- The search box matches this page on the section names `Gutter`, `Suggestions` and `Debugger` only
  (`Page::sections` in `settings.rs`). `Notebooks`, `Indentation`, `Files` and `Updates` are drawn but not
  searchable by name.

---

## Tools › Terminal

### Font

| Label | Control | Options / range | Default | Key |
|---|---|---|---|---|
| Size: | Dropdown, named `Terminal font size` | 10, 11, 12, 13, 14, 16, 18, 20. The file accepts 6 to 48 | 13 | `terminal.font.size` |

Help text: "The size of one cell in the terminal grid. Changing it tells the running program the new size."

### Shell

| Label | Control | Default | Key |
|---|---|---|---|
| Program: | Text field, 240 wide, named `Terminal shell` | empty (PowerShell on Windows, `$SHELL` elsewhere) | `terminal.shell` |

Help text (two notes): "The program each tab runs, started in the folder the explorer is showing." / "Leave it
empty for PowerShell." On macOS and Linux the second note says "Leave it empty for the shell named in $SHELL."

### Where a tab reopens

| Label | Control | Default | Key |
|---|---|---|---|
| Ask PowerShell where it is | Tick box | off | `terminal.shell_integration` |

Help text (two notes): "A tab reopens in the folder its shell was in, read off the shell's own process. That
answers for cmd.exe, bash and zsh and cannot answer for PowerShell: Set-Location moves PowerShell's location and
never the process's current directory." / "On, Unluminous adds one line to the prompt, after your own profile
has set it up, so the shell says where it is. Off, nothing about your shell is changed. A shell that already
reports its folder is followed either way."

### Not built yet / not on this page

- The search box does not match the `Where a tab reopens` section name (`Page::sections` lists only `Font` and
  `Shell`).

---

## Tools › MCP

### Install

Help text above the buttons: "Writes Unluminous into the agent's own configuration. The agent then launches
Unluminous's server itself over a pipe, so nothing listens on a port."

| Label | Control | Notes |
|---|---|---|
| `Install for Claude Code` / `Remove from Claude Code` | Button, 196 wide | The label depends on whether Claude Code's configuration already has Unluminous |
| `Install for Codex` / `Remove from Codex` | Button, 196 wide | The same for Codex |

These write to the agent's own configuration, not to Unluminous's settings. Under the buttons: before any press,
"Restart the agent afterwards. In Claude Code, `/mcp` then lists Unluminous's tools."; after a press, what the
button did.

### Server

| Label | Control | Options / range | Default | Key |
|---|---|---|---|---|
| Also serve over HTTP on this machine | Tick box | | off | `mcp.enabled` |
| Port: | Text field, 84 wide, named `MCP port` | 1024 to 65535; only a whole number is taken | 7345 | `mcp.port` |
| Tools: | Dropdown, named `MCP tool shape`, on the same row as Port | `One tool an area`, `One tool a command` | `One tool an area` | `mcp.tools` (`grouped`, `every`) |
| Areas: | Text field, 300 wide, named `MCP areas` | comma separated area names | empty (every area) | `mcp.areas` |

Help text (three notes, in order):
- "Which parts of the catalogue this offers, such as editor,git. Empty offers everything."
- "`<N>` tools. One an area names every command Unluminous has for about a third of the context one a command
  costs; one a command lets an agent permit a single tool by name." (N is counted live.)
- The server's state, one of: "Not listening. Agents that launch Unluminous's server themselves still work." /
  "Listening at `<url>`" / "Port `<n>` is already in use (`<reason>`). If that is another Unluminous, its server
  drives this window too." / "Unluminous was started with --control off, so there is no command channel for an
  MCP server to drive."

### Configuration

Help text: "For an agent with no button above. Paste it into that client's own configuration file."

| Label | Control | Notes |
|---|---|---|
| `Claude Code`, `Codex` | Two choice buttons, 118 wide. The chosen one is filled with the accent | Default `Claude Code`. Choose which block shows |
| `Copy` | Button, 60 wide, at the right of the same row | Copies the block |
| (block) | Read only monospace block, 560 wide, named `<client> configuration` | JSON for Claude Code, TOML for Codex |

Nothing in this section is a stored setting.

---

## Plugins

This page has no group heading. It is a plugin list rather than a page of settings, and none of it is written to
the settings file.

- Two tabs: `Marketplace` and `Installed` (with a count).
- `Search plugins` field.
- A list of plugins on the left, 300 wide.
- The chosen plugin's page on the right: vendor, an action button, `ENABLE` / `DISABLE`, an `Overview` heading
  and a `What it does not do` heading.
- The action button says `INSTALL`, `CUSTOMISE` (a bundled plugin that has not been written out) or `UNINSTALL`.
  Help text under it is one of:
  - "Installed with Unluminous and working. Customising writes its folder out where it can be edited by hand."
  - "Its folder is under the settings folder and can be edited by hand. Uninstalling removes it."
  - "Its folder is under the settings folder and can be edited by hand. Uninstalling takes that folder away and
    puts the plugin back to the one Unluminous ships."

---

## Plugins › Agent-Chat

Stored in `plugins/agent-chat/settings.conf` (`Configuration` in
`crates/unluminous-app/src/services/agent_chat/mod.rs`). The page scrolls inside its own scroll area. Labels in
the left column are 96 wide; fields are 26 tall.

### Endpoints

Help text: "What answers. `claude-cli` and `codex-cli` run the agent already installed on this machine: they need
no key, they bring their own tools, and they read the open project's own instructions. `openai`, `anthropic` and
`responses` send to a URL instead — for those a key is never stored by Unluminous, so name the environment
variable it is in."

A list of endpoint cards. Each card, top to bottom:

| Label | Control | Options / range | Shown for | Key (N is the row number) |
|---|---|---|---|---|
| (name) | Text field, 80 to 200 wide, named `Endpoint name` | | every row | `provider.N.name` |
| `Use` / `In use` | Button `Use`; the row in use shows a filled `In use` pill instead | | every row | `chosen` (the name of the row in use; empty means the first) |
| `Remove` | Button | | every row, except when there is only one row | (removes the row) |
| Speaks | A row of choice buttons, 74 wide each | `claude-cli`, `codex-cli`, `openai`, `anthropic`, `responses`. A shape whose program is not installed is not drawn, except the one the row already uses | every row | `provider.N.wire` |
| Program | Text field, hint `claude` | | rows that run a program (`claude-cli`, `codex-cli`) | `provider.N.command` |
| URL | Text field, hint `https://…` | | rows that send to an address | `provider.N.url` |
| Model | Text field. Hint `the agent's own` for a program row, `the model's name` for an address row | | every row | `provider.N.model` |
| Key from | Text field, the name of an environment variable. Hint `ANTHROPIC_API_KEY` for `anthropic`, `OPENAI_API_KEY` otherwise | | rows that send to an address | `provider.N.key-env` |
| Most tokens | Text field | 64 to 200,000 | rows that send to an address | `provider.N.max-tokens` (default 4096) |
| (status) | A coloured dot and one line. Green: "Ready · found on this machine, and it holds its own account", "Ready · key set from $`<VAR>`" or "Ready · no key needed". Red: the reason it cannot be reached | | every row | (not stored) |

The card in use has an accent border; the others have a divider border.

Under the cards: an `Add endpoint` button (130 wide). It adds an `openai` row named `endpoint-N`.

Defaults (three rows ship):

| Name | Speaks | Program | URL | Model |
|---|---|---|---|---|
| claude | claude-cli | claude | | |
| codex | codex-cli | codex | | |
| local | openai | | `http://127.0.0.1:8080/v1/chat/completions` | local |

A shipped row whose program is not installed and which is not the chosen row is not listed, but stays in the
file.

### Answering

The section shows different controls depending on whether the row in use runs a program or sends to an address.

**When the row in use runs a program:**

Help text: "The chosen row is a program on this machine, so it answers with its own tools, its own sandbox and
its own account. It runs in the project this window has open and reads that project's own instructions."

| Label | Control | Options | Default | Key |
|---|---|---|---|---|
| May | Three choice buttons, 84 wide | `read`, `edit`, `full` | `full` | `permission` |

Help text under May, one sentence per value:
- read: "Look and answer, changing nothing. `codex` runs in a read-only sandbox; `claude` refuses any tool of its
  own that would change something."
- edit: "Change files in the project this window has open. `codex` is sandboxed to it; `claude` accepts its own
  edits without asking."
- full: "Anything at all: edit any file and run any command, with no sandbox and nothing to ask. This is what
  ships, because the agent on the other end is the one already signed in on this machine and trusted with this
  checkout."

**When the row in use sends to an address:**

| Label | Control | Options / range | Default | Key |
|---|---|---|---|---|
| Let the model use Unluminous's own commands | Tick box | | on | `tools` |
| …including the ones that run a program | Tick box | | off | `shell` |
| Tool rounds | Text field, hint `100` | 1 to 500 | 100 | `tool-limit` |

Help text under the first tick box: "On unless you turn it off. The model is offered Unluminous's whole command
catalogue as tools, so it can open a file, read the git status or run a search — through exactly the code a menu
entry runs. The commands that run a program of the model's choosing are the switch below, and that one is off.
The same switch is the first button in the composer."

Help text under the second: "A second switch, because typing into a terminal, starting a run configuration and
installing a debug adapter hand this machine's shell to whatever is on the far end of that URL."

Help text under Tool rounds: "How many times in one turn the model may ask for tools before the pane stops asking
again."

### Conversation

| Label | Control | Options / range | Default | Key |
|---|---|---|---|---|
| System | Text field (one line), hint `nothing` | | empty | `system` |
| History | Text field, hint `20` | 1 to 500 | 20 | `history` |

Help text under System: "Added after Unluminous's own line, which says which file is showing. The file's text is
never sent by Unluminous."

Help text under History: "How many conversations are kept."

### Not built yet / not on this page

- `provider.N.key-entry` (a keychain entry name for an address row's key) is read and written in
  `settings.conf` but has no field on the page (`services/agent_chat/mod.rs`, `Configuration::write`;
  `unluminous-chat/src/provider.rs`, `key_entry`).
- The model choice and the context ring in the chat pane's own `Chat settings` dialog (opened from the gear in
  the pane, `task-2235`) are not part of this Settings page.

---

## Plugins › Agent-Tasks

Stored in `plugins/agent-tasks/settings.conf` (`Configuration` in
`crates/unluminous-app/src/services/agent_tasks/configuration.rs`), except the key, which goes to the machine's
keychain. The page scrolls. Each setting is drawn as its name in strong text, the control under it (300 wide),
buttons to its right, and a sentence under that. Every text field has a `Copy` button beside it.

### The board

| Label | Control | Default | Key |
|---|---|---|---|
| Board file | Read only path in monospace, with `Copy` and `Reveal` buttons | the default board path in the settings folder | `database` (not editable on the page) |
| Default project | Text field with `Copy`, hint `the folder this window has open` | empty | `project` |
| Lease | Text field with `Copy`, hint `45`. Whole minutes above 0 | 45 | `lease` |

Help text under Board file: "One Inillucent file. Copy the path to back it up, `Reveal` to open the folder it is
in, or set `database` in this plugin's own settings.conf to move it."

Help text under Default project: "Where an agent is launched when a ticket names no project of its own."

Help text under Lease: "How many minutes the board waits to hear from an agent before the watchdog calls its
lease expired and starts nudging it."

### What a new ticket gets

| Label | Control | Options | Default | Key |
|---|---|---|---|---|
| Default agent | Two choice buttons, 84 wide | `claude`, `codex` | `claude` | `agent` |
| Default model | Dropdown (the same control a ticket's own Model uses) | Empty is `the agent's own default`. For claude: `claude-opus-5[1m]`, `claude-opus-5`, `claude-sonnet-5`, `claude-fable-5`, `claude-haiku-4-5-20251001`. For codex: `gpt-5.6-sol`, `gpt-5.6-terra`. A saved value that is not in the list is shown first | empty | `model` |
| Default effort | Dropdown | Empty is `the agent's own default`, then `low`, `medium`, `high`, `xhigh`, `max` | empty | `effort` |

Help text under Default agent: "Which agent a new ticket is assigned to, and which one takes a ticket assigned to
a person."

Help text under Default model: "Passed as `--model`. The models the chosen agent has, and whatever this setting
already names if it is not one of them."

Help text under Default effort: "Passed as `--effort`. Codex knows nothing above `high`, so `xhigh` and `max`
collapse onto it."

### How an agent is launched

| Label | Control | Default | Key |
|---|---|---|---|
| Claude command | Text field with `Copy`, hint `claude --dangerously-skip-permissions` | empty (Unluminous's own command line) | `claude-command` |
| Codex command | Text field with `Copy`, hint `codex` | empty | `codex-command` |

Help text under Claude command: "The program and the flags in front of it, for a ticket assigned to Claude. The
ticket's own model and effort and the session flag are added after it, because those come from the row and the
board has to be able to resume the conversation it named. No shell runs this, so nothing expands and a path with
spaces in it goes in \"quotes\"."

Help text under Codex command: "The same for a ticket assigned to Codex. `resume <id>` stays the first thing after
the program, because it is a subcommand rather than a flag."

### Where the agent connects

| Label | Control | Default | Key |
|---|---|---|---|
| Iliad URL | Text field with `Copy`, hint `the agent's own endpoint` | `https://iliad-emerging-api.abbvienet.com/api/llm` on a file that has never been written; empty once cleared | `base-url` |
| Iliad key | Password field (dots), its placeholder says where the key is: "set, from `<source>`" or "not set: type it here, or export ANTHROPIC_API_KEY in your shell profile". Buttons `Save key` (70 wide) and `Clear` (60 wide) | not set | keychain entry `iliad` under the service `unluminous-agent-tasks` (not in `settings.conf`) |

Help text under Iliad URL: "The gateway the agent talks to, handed to it as ANTHROPIC_BASE_URL and
OPENAI_BASE_URL. Empty means whichever endpoint the agent itself is configured for, which is api.anthropic.com
for Claude."

Help text under Iliad key (macOS and Linux): "Typed once and never shown again. It goes to this machine's
keychain under `iliad`, not to a file, and Unluminous reads it at the moment an agent is launched and hands it
over as ANTHROPIC_API_KEY, OPENAI_API_KEY and ILIAD_API_KEY. Nothing needs typing here when your shell profile
already exports ANTHROPIC_API_KEY: Unluminous reads that profile whether it was started from a terminal or from
the Dock, so an agent gets what a command typed in the terminal tile gets. The agent's own environment is where it
ends up, which any program running as you can read, so treat it as a key that machine holds."

On Windows the field and both buttons are not drawn. In their place: "not set, and Unluminous has no keychain on
windows: export ANTHROPIC_API_KEY in your shell profile", with the help text "There is no keychain here for
Unluminous to write to, so nothing can be typed. Export ANTHROPIC_API_KEY in your shell profile and the agent is
given it."

### Not built yet / not on this page

- **The Iliad key on Windows.** `services/agent_tasks/keychain.rs` can read and write Windows Credential Manager
  (`CredWriteW`, `CredReadW`, `CredDeleteW`), but the page decides whether to draw the key field with
  `cfg!(any(target_os = "macos", target_os = "linux"))`
  (`components/agent_tasks/settings_page.rs`, `has_a_keychain`), so on Windows the field is not drawn.
- `database` (where the board file is) can only be changed by editing `settings.conf`; the page shows it read
  only.

---

## Plugins › Database

Stored in `plugins/database/sources.conf` (`crates/unluminous-app/src/services/database/config.rs`). The page
scrolls.

### Data sources

Help text: "Where each one points, and where its password is kept. Unluminous never writes a password into a
file: it goes into this machine's own credential store — the keychain on macOS, the Secret Service on Linux,
Credential Manager on Windows — and what is written down is the name of the entry."

A list, one entry per data source. Each entry is two lines:

| Part | Control |
|---|---|
| Name | Text, strong |
| `Edit` | Button, 60 wide. Opens the Data Source dialog |
| `Remove` | Button, 78 wide. Removes the data source at once |
| Second line | Faint text: `<where it points> · <connected / connected, read only / read only / not connected> · password <where it is>`. The password part is `none`, `environment <VAR>`, `keychain …` or `typed, until this window closes` |

With no data sources the list says "There are none yet."

Under the list: a `New data source` button (150 wide). It opens the New Data Source dialog.

Stored per data source N: `source.N.name`, `source.N.engine`, and either `source.N.file` (SQLite, Inillucent) or
`source.N.host`, `source.N.port`, `source.N.database`, `source.N.user`, `source.N.sslmode` (PostgreSQL), plus
`source.N.password.env` or `source.N.password.keychain`.

### Rows

| Label | Control | Range | Default | Key |
|---|---|---|---|---|
| (field) `rows a page, in a grid and in a console result` | Text field, 90 wide, named `Rows a page`, with the words beside it | 1 to 100,000 | 200 | `page_size` |

Help text: "One more than this is asked for and thrown away, which is what makes `1-200 of 200+` honest: nobody
counted the rest."

### The Data Source dialog (opened by `New data source` and `Edit`)

A separate modal, 520 by 470, titled `New Data Source` or `Data Source`
(`components/database/modal.rs`). Listed because it is the only way to change a data source from this page.

Section `General`, with a `Test Connection` button and its result.

| Label | Control | Options | Default | Shown for |
|---|---|---|---|---|
| Name | Text field | | | every engine |
| Engine | Three choice buttons, 90 wide | `PostgreSQL`, `SQLite`, `Inillucent` | `PostgreSQL` | every engine |
| File | Text field and a `Browse…` button | | | SQLite, Inillucent |
| Host and port | Two text fields, `Host` and `Port` | | `localhost`, 5432 | PostgreSQL |
| Database | Text field | | | PostgreSQL |
| User | Text field | | | PostgreSQL |
| Password | Password field (dots) | | | PostgreSQL |
| Connection security | Three choice buttons | `Off`, `If offered`, `Required` | `If offered` | PostgreSQL |

Help text under File: "The path of a SQLite file. It is opened, never created - a mistyped path says so rather
than quietly making an empty database."

Help text under Password depends on where the password is now:
- none yet: "Kept in this machine's own credential store - the keychain on macOS, the Secret Service on Linux,
  Credential Manager on Windows. Unluminous writes no password to a file, ever: what the settings file records is
  the name of the entry."
- in the credential store: "There is a password kept for this data source in this machine's own credential store.
  Type a new one to replace it, or leave this empty to keep the one that is there."
- from an environment variable: "This data source reads its password from an environment variable, which is how
  it was set up. Type one here to move it into this machine's own credential store instead."

Help text under Connection security: "Whether the connection to the server is encrypted. Required refuses to
connect without it. This is about the connection, not about how your password is kept - that is always in this
machine's own credential store."

Footer: `Cancel` and `Save`.

### Not built yet / not on this page

- A data source can be marked read only (`source.read_only`, shown as `read only` on the second line), but
  neither the page nor the dialog has a control for it. `task-1795` removed the tick box; the only way in is
  `unluminous-cli plugins run database read-only`.
- `chosen` (which data source is selected) is stored in `sources.conf` but chosen in the Database pane, not here.
