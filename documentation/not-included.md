# Not included

What is deliberately absent, and the reason for each. The design documents under `tasks/` have the
full argument where there is one; this page is the list.

## In the editor

**Right to left and complex writing systems.** This version places one grapheme cluster after another
from left to right, which is correct for Latin, Greek and Cyrillic and wrong for Arabic and Hindi. The
font metrics boundary is where a shaping step would go.

**Several carets at once, and column selection.** Both are expected by anyone coming from the
reference editors and neither is here yet.

**Writing a file back in an encoding other than UTF-8.** Unluminous is a UTF-8 editor. It *opens* a
file with a UTF-16 byte order mark, and a file that is not valid UTF-8 at all — read as Latin-1,
because every byte has a meaning there and so the reading cannot fail — but both open **read-only**,
with the encoding named in the status bar, and saving one is refused rather than attempted.
Re-encoding somebody's file into a scheme this version has not been asked to get right is a worse
answer than saying no. UTF-8 with a byte order mark is ordinary UTF-8 and is written back with its
mark.

Line endings are **not** part of that bargain: what a file was read with is what it is written back
with, so a one character edit is a one line diff.

**Indenting a selection by more than one character.** The indent unit is a character, and applying the
command four times would be four undo steps. The fix is one field — a width on the command — and it
belongs beside the loop that would read it.

## In the colouring

A **regular expression literal**, which cannot be told from division without parsing. **Nested block
comments** in Rust. **Interpolation inside a template literal.** And **JSX**. Each plugin says so on
its own page in `Settings -> Plugins`.

## In code navigation

**A language server.** Go to definition, find all references and rename are a syntactic index built
from the token stream, which is the tier Sublime Text's goto-definition and GitHub's shipped code
navigation are. A language server would be the true answer and would die on most machines — a separate
program per language, found on `PATH`, holding gigabytes, and nothing about it could be a screenshot
test because when it answers depends on the machine.

**Auto-import.** A different feature with a different risk, since it edits a part of the file the caret
is not in.

**Bare package specifiers** in import completion, **`tsconfig` path aliases**, and **following
re-exports**. `node_modules` is out of the walk, and putting it back was measured: it took the file
list from 618 to 2,022 and the search from 20 ms to 60.

## In the Markdown preview

**Footnotes, reference style links, nested block quotes and HTML.** Tables **are** drawn — set in the
code font with the columns padded to line up — and a picture **is** drawn when it is the whole of a
line and it is a file on this machine. One inside a line of prose stays its alt text, because it would
need inline layout the engine has not got, and one with a scheme in front of it is refused, because
Unluminous makes no network requests.

## In diagrams

**Ten of Mermaid's thirty types** — C4, ZenUML, architecture, swimlanes, event modelling, Venn,
Ishikawa, Wardley, Cynefin and tree view — which are **named rather than drawn**. That distinction has
a test of its own: a type that is named is honest, and a type that is mis-drawn is not.

A diagram's own `style`, `classDef` and `click` directives are **read and ignored**: a document does
not choose the window's colours, and nothing in a diagram is going to run.

The pictures are **not pixel identical to `mermaid.js`**, because none of it runs `mermaid.js`. The
bar they are held to is correct and readable.

## In the terminal

**Images, the Kitty keyboard protocol, a blinking cursor, and searching the scrollback.**
`tasks/unluminous-terminal-tdd.md` lists them with the reason for each.

## In the debugger

**Attach.** Launch only; the protocol work is attach-ready and attach is its own ticket.

**More than one session at a time.** There is machinery for more than one *connection*, because
js-debug hands its target to a second session and that is not a feature but how it works at all — but
one program is debugged at a time.

**Python.** No Python debugger entry ships, because a `python` entry no manifest could name would be
dead code. The Python *language* plugin ships; what it does not name is a debug adapter.

**Smart Step Into, Force variants, stepping filters, Reset Frame, data breakpoints, memory and
disassembly views, and hot code replace.**

**Downloading adapters.** Nothing is fetched. Pressing Debug with no adapter installed is one sentence
naming what was looked for and the command that installs it, and pressing that button starts a visible
run configuration in the run tile rather than a silent download.

## In the browser tab

**A screenshot that contains the page.** A rendered page is a native child window the operating system
composites on top of the surface a screenshot captures, so `window screenshot` has never held one and
`space browser <node> shot` does not either. Measured rather than assumed.

**A JavaScript host bridge.** Local HTML uses a project origin, every requested resource is resolved
under the registered canonical root, traversal and write methods are refused, and nothing is exposed
to the page.

**A second native view.** A window has one, whatever the tab count. Creating a second while another
lives on the thread blocks in a nested message pump on a completion that never arrives — no crash, no
error, and no frame ever drawn again. Every other explanation was eliminated.

## In the chat pane

**A context meter.** A URL and a model name say nothing about a context length, so a bar there would
be a fraction of a number nobody measured. What is drawn is the tokens in and the tokens out, which
the server really reported.

**A key field for a program.** A row that runs `claude` or `codex` holds no key at all, so there is
nothing to draw.

**Running an agent's tool calls.** Unluminous runs none of them; what it does with one is show it.

## In the Database plugin

**Editing a row that cannot be addressed.** A grid is editable when its rows came from one table with
a primary key, or a SQLite table with a `rowid`; otherwise the Add, Delete and Submit buttons are
**absent** with one line saying why. The alternative — an `UPDATE` matching on every column — quietly
changes two identical rows.

**A read-only safety check in the window.** It was taken away on purpose: a new data source is
writable. The **guarantee** stays, because it was never a parser in Unluminous —
`SET SESSION CHARACTERISTICS AS TRANSACTION READ ONLY` and SQLite's read-only open flag are the
server's own, and `plugins run database read-only` is the one way to ask for them.

**A Stop button on an Inillucent source**, because that driver reports that it cannot cancel, and a
cancel that returned success and did nothing is worse than no button. Read from the driver at run time
rather than written down, so the day the engine grows a cancel the button appears with nothing edited.

## In the product as a whole

**A three way merge editor**, **a marketplace that fetches a plugin over the network**, and
**anything that is fetched at all**. Nothing is downloaded: not a plugin, not a debug adapter, not a
picture named in a Markdown file with a scheme in front of it, not telemetry, and nothing at startup.
A package manager somebody pressed a button for is not the editor reaching out, and neither is an
agent they pressed send on.

**A light theme.** Refused with the reason written down rather than implied: the window is drawn on a
transparent ground, the depth recipe lifts a surface and shadows it with black, and 483 accepted
pictures are judged against a dark ground. A light theme is not a palette swap.

**Screen reader support in 1.0.** The accessibility tree is on and every control has a plain name, so
the platform gets something; `design/accessibility.md` is what is still missing, what the contrast
really measures — 23 of 28 pairs meet WCAG 2.2 and the word on the primary button is 2.77:1 — and the
plain answer, which is no. `node tools/contrast.mjs` computes every ratio from the palette itself, so
it can be run again the day a colour moves.

**Continuous integration.** By choice: the release scripts run the suite before they will tag
anything, so a release cannot be made from a checkout whose tests do not pass, and that is the gate.
