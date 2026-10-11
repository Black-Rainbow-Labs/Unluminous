Settings screen 3 of 9: Editor › Editor
Code: components/settings_dialog.rs (editor_page). Keys are settings file lines.

Each setting: Label | control | options or range | default | key

**Gutter**
- Show line numbers | tick box | | on | editor.line_numbers
Help: "A number against each line of the file. Unluminous wraps, so a paragraph that runs over several rows is numbered once, against its first row. Right clicking the gutter puts the numbers away and annotates with git blame."

**Notebooks**
- Scroll long outputs | tick box | | on | notebook.scroll_outputs
Help: "An output taller than thirty lines scrolls inside its cell. Off, every output is drawn at its full height."

**Suggestions**
- Suggest completions as you type | tick box (ticked = automatic, unticked = manual) | | ticked | editor.suggestions
Help: "A list of names appears under the caret once two letters of a word have been typed, in a file whose language a plugin claims. Off, nothing appears until you ask: Ctrl+Space, or Complete Word on the Edit menu, which work either way."
- Ask language servers (rust-analyzer, tsserver) | tick box (ticked = automatic, unticked = off) | | ticked | editor.servers
Help: "Rust and TypeScript files ask the language server the project already has for the members of a value, the import a name needs and a call's parameters. Off, no server is started and the structural answers stand alone."

**Indentation**
- One indent is: | dropdown | Tabs, 2 spaces … 8 spaces | Tabs | editor.indent (tabs or spaces:N)
Help: "What the Tab key types where nothing is selected. Indenting a selection still moves each line by one character, which is what Tab over a selection has always done."
- Indent a new line like the one above it | tick box | | on | editor.auto_indent
Help: "Pressing Enter starts the new line with the whitespace the line it was started from begins with."
- Trim trailing whitespace on save | tick box | | off | editor.trim
Help: "Off by default, and never on a Markdown file, where two spaces at the end of a line are a line break."

**Debugger**
- Show value tooltip | tick box (ticked = automatic, unticked = manual) | | ticked | debug.value_tooltip
Help: "While a program is stopped, resting the pointer on a name shows what it holds, and a structure opens into its fields, which can be typed over. Off, nothing appears until you ask: Show Value on the Debug menu."
- lldb path: | text field, 300 wide | a path | empty | debug.lldb
- node path: | text field, 300 wide | a path | empty | debug.node
Help under both (one row per known debugger): "Leave empty to search the usual places; the debug tile offers to install one when none is found."

**Files**
- Line endings: | dropdown | Keep what the file has, Always LF (Unix), Always CRLF (Windows) | Keep what the file has | editor.line_ending (keep, lf, crlf)
Help: "What a file is written back with. Kept as it was found is what leaves a one character edit as a one line diff; the other two bring every file that is saved into line."
- Exclude: | text field, 300 wide | comma separated .gitignore patterns | empty | editor.exclude
Help: "Patterns Go to File, Find in Files, completion and Go to Definition leave out, separated by commas, written the way a .gitignore line is." / "The project's own .gitignore is read already; this is a list beside it. The explorer goes on showing everything either way."

**Updates**
- Check for a newer version once a day | tick box (ticked = daily, unticked = off) | | ticked | update.check
Help: "One request to unluminous.com a day, shared by every window. A newer version is offered in a notice with Install & Restart and Don't Ask Again. Off, Unluminous sends nothing until you ask: Check for Updates on the Unluminous menu works either way."

**Not built yet**
- update.check = "start" (check at each window open) exists but the tick box cannot choose it (settings.rs).
- update.skip (set by Don't Ask Again) has no control (settings.rs).
- Search finds only Gutter, Suggestions, Debugger here (settings.rs, Page::sections).
