# Writing code in it

[Editing](editing.md) is what Unluminous does with text whatever the text is. This is the half that
needs a language behind it, and the language comes from a plugin — see [Plugins](plugins.md) for what
is in one.

## Line numbers, tabs and colours

**Line numbers** down the left. Unluminous wraps, so a paragraph that runs over several rows on screen
carries one number against its first row and nothing against its continuations, which is what a line
number means everywhere else. Right clicking the gutter puts them away or annotates the file with git
blame.

**A tab for each open file**, carrying the icon of the plugin that claims it. A single click in the
explorer opens a file in the tab a single click reuses, drawn faintly to say so; a double click opens
it in a tab of its own, and so does typing into a tab you were only glancing at. `Ctrl+Tab` and
`Ctrl+Shift+Tab` move between them, `Ctrl+F4` closes one, and `Ctrl+Shift+T` reopens the last closed.

A closed tab is remembered as a path rather than as a document: one closed with unsaved changes has
already been written and one closed with `--discard` was discarded on purpose, so reopening means
reading the file again either way. Ten are kept, the same file twice is one row, and none of it is
written to disk — travel history rather than state.

**Colours** come from the plugin that claims the extension. Twelve language plugins ship: JavaScript,
TypeScript, Rust, Python, CSS, HTML, JSON, YAML, TOML, SQL, shell and Mermaid. A file over two
megabytes is left as plain text with a line in the status bar saying so.

## Completion

Type two letters of a word in a file a plugin claims and a list of the names it could become appears
under the caret. `Up` and `Down` steer it, `Tab` and `Enter` take a row, `Escape` puts it away, and
typing carries on underneath it the whole time. `Ctrl+Space` and `Code -> Complete Word` ask for the
same list by hand.

**Most of what it offers was already in memory**, which is what makes it a small feature where most
editors' completion is an enormous one. Four sources: this tab's definitions and its distinct words,
the other open tabs' definitions, the project's symbol index, and the language's own keywords,
builtins and types out of the plugin manifest. No new index, no watcher, no debounce. There are three
tiers of answer. The words of the file and the project come first. Behind them is the structure of the
project, which the code index reads from each language's definitions, and behind that is a language
server, when one is switched on (see [Language servers](#language-servers)). The words and the
structure answer first, every time, and the server's rows are merged in when they arrive. Nothing
waits for a server.

**A `.` or `::` opens the list with nothing typed.** A plugin's `language.members` names the characters
that do this (`.` and `::` for Rust, `.` and `?.` for TypeScript and JavaScript, `.` for Python). The
list is the members of the receiver. The receiver's type is read from the enclosing `impl` or class
when the receiver is `self` or `this`, from a parameter or `let` with a written type, from a field, or
from a constructor call on the same line. Generics are stripped and references are looked through.
When the type is not known, the rows are the members of every container whose name matches the stem,
each marked with its container. Typing `(` or `,` inside a call shows the callable's signature on a
line above the caret with the parameter being typed in bold, drawn from the server when one runs and
from the project's definitions otherwise. `unluminous-cli editor signature` answers the same
question. The list has a documentation panel beside it for the chosen row.

**A name from another file brings its import with it.** Accepting a row for a name the project exports
from a file this one does not import writes the `use` line in Rust or the `import` line in TypeScript,
merged into an existing one from the same module when there is one, in the same undo step as the name.
When two files export the same name, both rows are shown, each marked with its file, and nothing is
guessed.

The match is a case-insensitive **subsequence** and the score is Sublime Text's rubric — a large bonus
for a prefix, one per matched letter on a word boundary, one per consecutive letter, a small one for
matching case, and a penalty per unmatched letter so the shorter of two names wins. The alignment is
the **best** one rather than the first, because `pt` reads `paint_text` two ways and only one of them
is the one a person meant.

**The row equal to the stem is never offered**, and that one rule is what makes `Enter` safe. VS Code
grew a three-way setting because people pressed `Enter` meaning "new line" and got a suggestion;
dropping the no-op row answers it at candidate time instead, so once a word is completely typed the
list has either something genuinely longer to offer or nothing at all.

`Tab` replaces the whole identifier and `Enter` replaces the stem, which is the reference editor's own distinction
and is right in both directions. `editor.suggestions` is `automatic` or `manual`; `manual` is already
the off switch, since `Ctrl+Space` works either way, which is why there is no third value. The
members of a receiver obey it too.

**The order is learned.** The rows are ordered by a ranking trained on positions taken from real code,
which reads how the typed letters matched the word, which sources offered it, its kind against the
place the caret is in, how near its last use is, and the server's own order. One ranking is read by the
popup and by `unluminous-cli editor complete`, and `editor complete --explain` shows every fact the
ranking read about each row and the score it gave. The case of a word's first letter is one of those
facts. `editor complete --stem <text>` ranks a hypothetical word without typing it.

### Language servers

`editor.servers` is `off` by default. Off starts nothing, stops a server that is running as soon as
the setting changes, and completion answers from the project's own structure. `automatic` starts
`rust-analyzer` for Rust and `tsserver` for TypeScript and JavaScript when one is found, from the second
frame on, once a file whose plugin names a server is showing. A plugin names its server with the
`language.server` key, which names an adapter built into Unluminous (`lsp` or `tsserver`), and a name
this version does not have is refused when the plugin loads.

The setting is off because of what a server costs. `rust-analyzer` typically holds 1 to 4 GB on a
project the size of this one and starts `cargo` and `rustc` while it reads the workspace. The Editor
page says so beside the switch.

`rust-analyzer` is looked for in the project's toolchain, on `PATH`, and from `rustup`, and `tsserver` in
the project's own `node_modules/typescript` or a global install. When none is found its state is
`Absent` with what would install one. The state is shown beside the language in the status bar, for
example `rust-analyzer: ready`, and `unluminous-cli status --section servers` says what each server is
doing. Nothing is written to the project folder by a server, and the window never waits on one.

### Inside an import, the list is the files, and what they export

Start writing `import { } from '` and the list under the caret is the project's own files; put the
caret between the braces and it is what that file exports. `use unluminous_core::comp` offers the same
thing walked down a module tree instead.

One question is asked before the four sources are gathered: *is the caret in the middle of an import?*
When the answer is yes the four sources are **not** gathered at all, because a keyword, a local word
and an unrelated name from the project are all wrong answers to `from '│'`.

Two families, because there are two shapes and no third: a **quoted** module is a string resolved
against the file system, and a **path** module is segments resolved against a module tree. Both are
read backwards from the caret — a few hundred bytes of scanning a keystroke rather than a reading of
the file, and a half-typed line above cannot poison the answer. The path family's walk **is** its
parse, and the keyword it ends at is the whole of what makes it trustworthy: `use` in front and it is
an import, anything else and `a::b::c` is ordinary code.

The tier is syntactic and says so. What is offered is what is really there — the files come from the
same list `Go to File` searches, so a specifier Unluminous offers is one that really resolves and
nothing outside the project can be reached. **The inserted specifier is always relative**, because it
is the one spelling that is always right, needing no `tsconfig.json`, no `baseUrl`, no alias table and
no `exports` map.

## Go to definition, find all references, rename

`Ctrl/Cmd+Click` or `Code -> Go to Definition` goes to where a name is defined. `Alt+F7` lists every
reference. `Shift+F6` renames one everywhere it is used. Each of these can be given another shortcut
on `Settings -> Keymap`, or by choosing the VS Code or IntelliJ preset.

`Ctrl/Cmd+Click` jumps in whichever pane is under the pointer, and not only in the pane that has the
keyboard. A name that no definer declares, which is a parameter, a local or a field, goes to where it
is first written in its function; a field goes to the first place it is written without a `.` in front
of it.

**Back and forward are `Ctrl/Cmd+[` and `Ctrl/Cmd+]`** (`Edit -> Navigate Back` and `Navigate Forward`),
and the older `Ctrl/Cmd+Alt` with the arrow keys still work. The brackets are never taken from a
terminal, where `Ctrl+[` is `Escape` and `Ctrl+]` detaches from `claude`. Opening a file, `Go to File`,
`Go to Line`, a search hit and a jump to a definition all record the place you left, and a place a few
lines from the last one replaces it. The history is not written to disk.

**The tier for navigation is a syntactic index**, built from the token stream the colouring already
produces, and the three mechanisms were weighed. Completion uses a language server when one is
switched on, because completion needs name resolution and type inference. Go to definition, references
and rename do not use one. A language server client would be the true answer for them and would die on most
machines — a separate program per language, found on `PATH`, holding gigabytes, and nothing about it
could be a screenshot test because when it answers depends on the machine. Tree-sitter is code where
Unluminous's plugins are data. This is the tier Sublime Text's go to definition and GitHub's shipped
code navigation are, and it is what makes the answer instant, deterministic and testable with no
window.

**What a definition is comes from the plugin, not from a list of languages in Unluminous.** Two
manifest keys, both off unless a language asks for them: `language.definers` is a comma list of
`keyword=kind`, and `language.brace_definitions` turns on the one heuristic, for the definition Rust
never hides but JavaScript and TypeScript do — a class method has no keyword in front of its name.

**Honesty is the whole of the design.** Where the mechanism cannot tell two same-named things apart it
shows both rather than guessing one; a definition found by the brace heuristic is marked as likely and
stays marked all the way to the screen; and an occurrence inside a comment or a string carries the
role that says which, is listed second, in the quiet colour, and is never ticked by default in a
rename.

**Definitions are indexed and occurrences are not.** The index holds `name -> where it is defined` for
the project, built on a worker thread. Find all references is a **search** instead, in a whole-word,
role-classified mode of the same searcher: an index of every occurrence would buy nothing at this size
and would cost the one thing a search never pays, which is invalidation — a build, a branch switch or
another editor moving a file would all have to be noticed.

One rule settles every awkward case: *a file that is open is owned by its document, and every other
file is owned by the index.* An open tab's definitions come from its live text, the index's copy of an
open file is never offered beside it, and a reference search is handed the text of the tabs rather
than the bytes under them. The disk-owned side is **re-checked at the moment of use** rather than
watched: before jumping into a closed file its text is read again and the name confirmed to still be
there.

**The rename modal is the preview, and the tick boxes are the change set.** What is applied is exactly
the ticked rows. An open file is edited as a document — one command, which is one undo step by
construction — and is left with unsaved changes rather than being written, because a rename must never
silently write a buffer somebody was editing. A closed file is read, every ticked range is checked to
still hold the old name, and only then is it written once; a file that changed since the search is
skipped whole and reported by name rather than patched on faith. A collision is a **warning**, not a
refusal: the mechanism cannot know whether it shadows, so it says what it does know.

The three entries are **absent** when the file's language cannot answer them. That absence is also
what lets the command key and `B` mean bold in prose and `Go to Definition` in code: the two questions
are true of opposite files, so the two can never both fire on one press.

`cargo run --release -p unluminous-app --example symbol_cost` measures the lot: 155 files indexed in
38 ms, 11,497 definitions, a reference search over 176 files in 42 ms.

## The line commands

A comment toggle, duplicate, move, join and sort a line, `Go to Line`, and the matching bracket. Every
one is a single command applied through one function, so each is **one undo step by construction**.

What the window adds is the three things the crate deliberately does not know: which marker this
language comments with, whether a setting says to do it, and what a person sees. The marker comes from
the plugin, and a language that names neither a line comment nor a block comment gets **no entry** —
CSS will never have a `//`, and it keeps its block comment entry while losing the line one.

**A chord is checked against the menus before it is bound**, and two of the obvious ones were not
free: `Cmd/Ctrl+D` is Git's `Show Diff` and `Cmd/Ctrl+G` is `Find Next`, so Duplicate Line is
`Cmd/Ctrl+Shift+D` and Go to Line is `Cmd/Ctrl+L`. Moving a chord somebody already has in their
fingers so a new feature can have it was weighed and refused.

`editor.indent` says what one indent is, which is what the `Tab` key types where nothing is selected.
Indenting a **selection** still moves each line by one character, because the crate's indent unit is a
character and applying the command four times would be four undo steps.

`editor.trim` is off and **never runs on Markdown**, where two spaces at the end of a line are a line
break, so trimming them changes what the document means rather than tidying it. It runs in the one
place a tab is written, so a save from the menu, from `Ctrl+S`, from `tab save` and from closing a
modified tab all do the same thing — and it is an ordinary command, so a person who did not mean it
can undo it.

## The keymap

`Settings -> Keymap` chooses a preset (`unluminous`, `vscode` or `intellij`), and a shortcut set on an
action's row is kept on top of it. A chord is checked against every menu entry and against the keys the
editing area reads for itself before it is bound, and a chord another action holds is taken from that
action with the answer saying so. [The window](the-window.md#the-keymap) has the rules, and
`unluminous-cli settings keymap` and `settings bind` are the command line.

## Searching the project with the code index

The code index answers what `ripgrep` would, from memory. It is Atrius, a separate project with its own
repository, which Unluminous depends on as a crate, and the same index is reached by the window, by
`unluminous-cli search` and by the MCP tool `unluminous_search`. It is built for each checkout in a cache
folder outside the project, one host for each checkout, started by the window, by `mcp serve` or by a
detached `search serve`.

- `search find <query>` gives the lines `ripgrep` gives for a literal or a regex, grouped by file and cut
  to a token budget. The file set is the one the Grep tool uses: hidden files included, version
  control folders left out, and `.gitignore` obeyed. A question in plain English goes to a table of
  passages taken from each language's definitions and folds.
- `search def <name>` is where a name is defined (`Type::name` works), `search refs <name>` is its uses,
  `search fragment path:line` is the function around a line, `search outline <path>` is a file's
  definitions, and `search files <name or glob>` finds files.
- A change on disk is in the next answer. Writing, editing, renaming, deleting, a burst of 5,000 files
  and a branch switch gave no stale or missing results against `ripgrep` on the three local
  repositories measured.
- On the three local repositories `find` is 27 to 36 times faster than `ripgrep`, with the same lines.

`search.embeddings` is off by default, so plain English questions are ranked by their words. Turning it
on loads the `nomic-embed-text` model, which holds about 1 GB while it is loaded and is let go ten
seconds after its last use. The Editor page and `settings` say what the switch costs. Nothing is
downloaded for it automatically. `search.model_kept`, `search.embedding_threads` and the other
`search.*` settings are listed by `unluminous-cli settings list`.

## Notebooks

A `.ipynb` file opens as a notebook tab: code, Markdown and raw cells, run against a Jupyter kernel,
with each cell's outputs drawn under it. `File -> New Jupyter Notebook` and `File -> New Rust Notebook`
make one, so does `New -> Jupyter Notebook` on an explorer row, and `unluminous-cli notebook convert`
turns a `.py` file's `# %%` cells into a notebook, or a notebook into a `.py` file, beside the original
and without writing over anything.

**The tab's document is the cells written as percent text.** Each cell is a marker line followed by
its source: `# %% id=X` for code, `# %% [markdown] id=X` and `# %% [raw] id=X` for the others. The
caret, the selection, undo, find, multiple carets, completion, colouring and the gutter all work on a
notebook without knowing it is one, because they work on a document. The marker lines are hidden. What
is drawn between the cells is room in the layout and not text: the outputs and a status line sit under
the last line of a code cell, and a rendered Markdown cell or a collapsed cell is one empty line of the
height it needs. A notebook saved without changes comes back byte for byte, including Windows line
breaks, and each cell's outputs, metadata and execution count are kept by its id through edits.

**There are two modes, as in the reference editor.** Command mode works on whole cells: `A` and `B` add
above and below, `D D` deletes, `Z` undoes the deletion, `M` and `Y` make a cell Markdown or code, `C`
and `V` copy and paste, and `Shift` with the arrow keys extends the choice. Edit mode is ordinary
typing. Backspace at the start of a cell and Delete at the end of one do not join a cell to its marker.
`Notebook -> Cell` holds the same commands, and the notebook has an outline and sections that fold.

**The kernel is spoken to through Python, and there is no ZeroMQ in Rust.** A Python program compiled
into the binary starts the kernel with `jupyter_client` and speaks JSON lines to Unluminous, so any
installed kernel spec works. The Pythons are looked for on a thread: the project's `.venv`, `venv`,
`env` and `.env` folders, `$VIRTUAL_ENV`, `$CONDA_PREFIX`, `python3` and `python` on `PATH`, the `py`
launcher, and `miniconda3` and `anaconda3` in the home folder. A Python without `ipykernel` gets a
banner with an `Install` button that runs `pip install ipykernel` in the run tile. One cell runs at a
time, and a cell that fails skips the cells still queued, as Jupyter does. Interrupting a single long
`time.sleep` does not work on Windows, which is a limit of `ipykernel` there.

**Debug Cell uses the debugger Unluminous already has.** The kernel contains `debugpy`, so the existing
debug session attaches to a port the kernel opens, and each cell is debugged as its own file. The
session only disconnects when stopped, so the kernel keeps its variables. Debug Cell is for Python only,
and its button is absent for other languages.

**Rust notebooks run on `evcxr`** through the same bridge. `evcxr_jupyter` registers a kernel named
`rust`, and the kernel picker offers `Install the Rust kernel (evcxr)`, which runs the install command in
the run tile. State between cells is `evcxr`'s own. The notebook's language is in its metadata, so
completion, the comment toggle and the symbol reading use the cells' grammar.

**Completion in a notebook asks the kernel.** After `.` or `::` in a code cell the kernel is asked with
nothing typed, and only its rows are offered there. `Tab` after a word, a `.` or a `:` asks for
completions in a code cell, which is Jupyter's own key. A kernel's range is its own, so its matches are
fitted to the editor's stem.

`unluminous-cli notebook` is the command line half: `status`, `cell`, `run`, `add`, `source`, `edit`,
`select`, `view`, `output`, `outline`, `kernel`, `variables`, `input`, `export`, `new`, `convert` and
`complete`. Everything a person does with the pointer has one, including collapsing cells and
outputs, sorting a table output, and `run --debug`. `notebook run --wait` answers when the cells
finish.

## Git

In the `Git` menu, in the same submenu on any explorer row, and in three places you do not have to ask
for: the branch and how far it is from its upstream in the status bar, each file in the explorer
tinted by what git thinks of it, and a change bar in the gutter against each line that differs from
the version git has.

**Unluminous runs the `git` program rather than using a library**, and the reason is what the
machine's own git already knows: a credential helper, an ssh agent, `commit.gpgsign`, hooks,
`safe.directory`, an identity for this repository in particular. A push from Unluminous has to be the
same push you get in the terminal. The cost is that the output has to be read, which is answered by
asking for the formats git provides for being read — `--porcelain=v2 -z`, `--line-porcelain`,
`--format` with the record separators — never the ones meant for a person.

Two rules follow:

- **Nothing invents an error message.** Every call returns git's own standard output and standard
  error whether it worked or not, and that is what the status bar shows. A rejected push, a merge
  conflict, a detached HEAD and a missing upstream all explain themselves better than Unluminous
  could.
- **Every command runs on a thread, one at a time.** Not because the window would be slow — because it
  would stop drawing until git finished, which on a fetch looks exactly like a crash. One at a time,
  because two commands at once in one repository fight over `index.lock`.

`Commit...` opens a panel with a changes tree, a tick box per file, the repository's row carrying its
branch, an `Unversioned Files` group, `Amend`, the counts, the message box with the last twenty
messages behind a button, and `COMMIT` and `COMMIT AND PUSH...`. **Ticking a file stages it at once**,
so Unluminous's idea of what is staged and git's cannot disagree while the panel is open.

`Rollback`, a hard `Reset HEAD` and dropping a stash each ask first, because none of them can be
undone. Pushing with force always uses `--force-with-lease`. A merge or a rebase that stops on a
conflict is not hidden: the status bar says so, the conflicted files are marked, the Git menu grows
`Continue` and `Abort`, and the file opens with its markers in it — which is a file holding text, and
therefore something Unluminous already edits.

## Running

A run configuration is a **named command line**, a folder and some environment variables — one kind,
not a template per language, because the surveyed templates all compose into one command line wearing
six boxes. Pressing the play button at the right of the title bar spawns a terminal session with the
program in place of the shell, so the output is a real terminal and stopping is killing a process
Unluminous owns.

**No shell runs the command line.** It is split the way a shell splits a double-quoted word and the
parts are handed to the process as arguments, so nothing expands, nothing globs, and `&&` is one
program with a strange argument rather than two programs. A backslash is a backslash unless it is in
front of a quote, because half the paths on Windows have one in them. Somebody who wants a shell
writes `pwsh -Command ...` and has said so where it can be seen.

Configurations live in `.unluminous/run-configurations.conf`. A **temporary** one — what running a
file or a suggestion makes — is capped at five and deliberately never written down, because a file the
project shares should hold what somebody chose to keep; `Save` in the dialog promotes one.

**Stopping is soft then hard.** The first press is the interrupt byte down the pseudoterminal, which
the program can catch; a program still alive two seconds later, or a second press, is killed. A run
records what it ended with the moment it arrives, because a program Unluminous killed has no code to
be asked for afterwards — and the code goes in the tab's **strip**, never into the grid, because a
line pretending to be program output is the confusion a separate strip avoids.

**Plugins contribute data, not types.** The answer to "should running node mean a Node plugin" is no:
node is how JavaScript runs, and the JavaScript manifest says so itself with `run.file = node {file}`.
`run.project` names a detector **built into Unluminous** — `cargo` reads `Cargo.toml` and `npm` reads
a `package.json`'s scripts — so the most a third-party manifest can do is suggest text, visibly.

`unluminous-cli run` is the whole feature from the command line, and `run output` is the one to
notice: it reads the run's scrollback rather than its screen, so an agent can start a dev server, read
its port out of the log, exercise it and stop it with nobody watching.

## Debugging

Click the gutter to put a red dot on a line, press `Shift+F9` to start the configuration the play
button starts **under a debugger**, and the program stops there: the line is marked, the call stack
and the variables are in the debug tile along the bottom, and `F8`, `F7`, `Shift+F8` and `F9` step
over, into, out and on. Double click a value to change it in the running program, `Alt+F8` evaluates
any expression, and while the program is paused each local's value is painted at the end of the line
that names it.

**One client, every language.** Unluminous speaks the Debug Adapter Protocol, and a debug adapter is a
separate program that speaks it on one side and drives a real debugger on the other. The debuggers
have already made this choice themselves — lldb ships `lldb-dap` inside every LLVM distribution,
Python's debugpy *is* an adapter, Microsoft's js-debug publishes a standalone server, Go's delve
serves it natively — so speaking it is not adding a translation layer, it is speaking the native
protocol of the programs that already exist.

Which debugger a language uses is one line in its plugin and the code that drives it shipped with the
binary. **Nothing is fetched**: pressing Debug with no adapter installed is one sentence naming what
was looked for, where it comes from, and the command that installs it — and pressing that button
starts a **temporary run configuration**, so the install is a visible program in the run tile that
`run output` reads and `run stop` stops.

An adapter is looked for where installers really put it, not only on `PATH`: the VS Code family's
extension folders, `C:\Program Files\LLVM\bin`, Visual Studio's bundled LLVM, homebrew, Xcode,
`/usr/lib/llvm-*` and Debian's versioned names. Versions sort by their numbers rather than as text,
because `1.11.4` sorts under `1.9.0` as a string and the answer would be a year-old adapter on a
machine that has both.

**Debug is Run, under a debugger** — the same configuration, same command, same folder, same
environment, which is the reference editor's own model. The debuggee runs **in the run tile**, through the
protocol's `runInTerminal` request, so it gets a real pseudoconsole with its colours and its
interactivity.

A configuration whose program is `cargo` runs a build tool rather than the program, so `cargo run` is
**built and then debugged**: the command is rewritten to `cargo build --message-format=json-…`, run on
a thread, and the binary is taken out of cargo's own artifact lines. Deriving `target/debug/<crate>`
by convention instead is wrong for workspaces, examples, tests, custom profiles and renamed binaries;
asking cargo costs one process and is always right.

**Breakpoints live where the marked passages live**, inside the document as the byte offset of each
line's start, so the two functions that know a range of bytes moved shift them in the same two lines.
They survive a restart in `.unluminous/breakpoints.conf`, carry a condition or a message to log
instead of stopping, and toggling one is **not an edit**.

**Unluminous draws the adapter's answer rather than its own hope.** The adapter says where each
breakpoint really landed and whether it is verified; one it moved is drawn where it put it, and one it
could not bind stays **hollow** for the life of the session. A breakpoint switched off is hollow too,
and **dimmed**, because a person's own decision and a debugger's refusal are not the same thing.

**Every optional feature asks the capabilities first**, so Unluminous never sends what the adapter did
not offer and a control whose capability is absent is absent. **Every variable reference dies on
resume**, which the protocol says and which is written down once; what is *not* thrown away is which
rows were open, remembered by their path of names rather than by their reference, so stepping through
a loop does not re-collapse the structure being watched.

`unluminous-cli debug adapters` is the doctor and is what an agent runs first: every debugger this
version drives, where each one really is, what is missing, the languages that use it and the command
that would install it.

One honesty that belongs on the page rather than hidden: with the MSVC toolchain rustup installs by
default on Windows, LLDB reads PDB debug information incompletely. Breakpoints and stepping work, and
some enums and collections render poorly. The session says so once when it starts, and the variables
tree shows what the adapter says rather than pretending.
