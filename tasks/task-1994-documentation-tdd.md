# task-1994: the README, the documentation folder, and the pictures

> Our readme is to long and needs better organization, better screenshots, reference to
> unluminous.com.
>
> Reference how our Inillucent documentation is written and organized, and follow a similar layout,
> breakdown, and writing style.

Three asks, and the first one is the smallest of them. `README.md` is 1,106 lines and 82 KB against
Inillucent's 377, which is the complaint as filed. What measuring it found is that length is a
symptom: the file has been added to for fifty releases without ever being re-read from the top, so
the figures in it have gone stale, two of its sections disagree with each other, and the newest half
of the product is not in it at all.

## 1. What is actually wrong

### 1.1 It is four documents in one file

The first third is a product page. The rest is the internals: the crates, the rope, what one frame
does, the threads, where state lives, what a frame costs, the plugin manifest key by key, the
tokeniser's rule order, the wire format, the four test layers. All of that is true and worth having,
and none of it is what somebody opening the repository for the first time is looking for.

The file says so itself, in its fourth paragraph: *"The first half of this file is what Unluminous is
and how to use it. The second half is how it is built."* A file that has to tell the reader where to
stop is two files.

### 1.2 Its figures have gone stale, and two of them contradict each other

Every number below was measured against the checkout at 0.50.0 rather than read off another
document.

| The README says | It really is | How that was measured |
|---|---|---|
| five crates | **eight** | `crates/` holds `core`, `terminal`, `git`, `dap`, `db`, `chat`, `app`, and `unluminous-cli` is beside them |
| ninety-seven commands | **213** | `unluminous-cli mcp tools --count` |
| fourteen area tools | **28** | the same command |
| nine bundled plugins | **16** | `crates/unluminous-app/plugins/` — 12 languages, 3 panes, 1 theme bundle |
| six bundled plugins, in a different section | **16** | the two sections disagree by seven |
| five menus | **seven** | `unluminous --print-menus`: `Find` and `Run` are both missing from the table |
| the MCP preamble costs about a third of the whole | 27,011 tokens against 58,579 | `unluminous-cli mcp tools --count` |

The two plugin counts are the part to take seriously. One section says six and another says nine,
nine lines of prose apart, and both are wrong. That is not a number that fell behind; it is a fact
written down in two places, which is the failure this repository already has a rule against
everywhere else.

### 1.3 Half the product is missing from it

Searching the file for the name of each feature:

| Feature | Times named in `README.md` |
|---|---|
| The Base of Infinite Space, the canvas, nodes | **0** |
| Auto-completion, completing an import | **0** |
| The Agent-Chat pane | 1, in a list of plugin ids |
| The Agent-Tasks board | 1, in the same list |
| The Database plugin | 1, in the same list |
| Themes | described, correctly |

A reader of this README does not learn that Unluminous has an infinite canvas holding terminals, web
pages, folder trees, file editors, chats and a ticket board wired to each other; that it completes
identifiers and import specifiers as they are typed; that it has a chat pane that runs the `claude`
or `codex` already installed on the machine; or that it reads PostgreSQL, SQLite and Inillucent
databases in a pane of its own. Those are not small features. Three of them have a design document
apiece.

Meanwhile *Find and Replace* is described under **Not included**, in a paragraph that begins by
correcting itself — *"this entry used to say it was not"*.

### 1.4 The pictures are of 0.1.0

`documentation/overview.md` is twenty-four captures taken on 2026-08-25 from **0.1.0**. The shipping
version is **0.50.0**. The page is honest about it — it opens with a list of what has changed — but
what it lists is most of what a picture shows:

- Every picture is in the `classic` icon set. `material` is the default now, and `task-1949` then
  redrew ten of its marks against named published icons.
- Themes did not exist. There are six now, and the one in the pictures is the default.
- The Base of Infinite Space, the Agent-Chat pane, the Agent-Tasks board, the Database plugin, the
  browser tab, the debugger, the run tile and folding all arrived afterwards, so none of them is in
  any picture.

`documentation/README.md` records why they were never re-taken, and the reasons are all still true:

1. **The capture scripts are in `_agent_output/task-1658-screenshots/`**, which is gitignored, so they
   exist on one machine and in no checkout.
2. **They press keys with `keybd_event`.** `CLAUDE.md` bans that outright: a run that stops between a
   key going down and its coming up leaves that key held for the rest of the session, with nothing on
   the screen to say so.
3. **They bring the window to the front.** `task-1914` bans that: activating a window that is on
   another virtual desktop switches the desktop with it, which is a machine somebody else is using
   being taken away from them.
4. **They need a 3840 by 2160 screen with every other window minimised**, because a screen copy
   photographs whatever is really behind the window. The session that last tried had a 1024 by 768
   remote desktop.

They also still say `quill`, which is what the product was called then.

## 2. What Inillucent does, and which parts to copy

`inillucent/README.md` is 377 lines and it is laid out as a front door rather than as a manual:

1. **One bold paragraph** saying what the thing is and what it replaces.
2. **A row of links** — the site, the documentation, install, the docs in the repository.
3. **A facts table**, three columns: a claim, what was measured, and the page that carries the
   measurement.
4. **Install**, then a first database, then a first search — each a block of commands that runs.
5. **What it does**, as five short sections, each ending in an arrow to the page that goes deeper.
6. **For AI agents.**
7. **In production** — one measured case.
8. **Documentation** — a table of every page with one line saying what it answers.
9. **Limits** — what it is not, and what is slower, in the author's own words rather than a reader's.
10. **Building it**, and **Licence**.

`inillucent/docs/README.md` is the other half: a **reading order** rather than an alphabetical list,
grouped into *Start here*, *The two engines*, *Using it*, *The measurements*, *Working on it*, and
*Case studies*, each group a table of `page | what it answers`. It states the rule the pages are
written to — *"Every page here is written to be read on its own. Nothing is left implicit because an
earlier page said it"* — and it is held to the directory by a test.

Four things are worth copying and one is not:

- **Copy the front door shape.** Headline, links, facts table, install, first use, what it does with
  arrows out, documentation index, limits, licence.
- **Copy the reading order.** Groups with a purpose, numbered, each row saying what the page
  *answers* rather than what it is called.
- **Copy the self-contained page rule.** A page states the fact it needs rather than pointing at
  another page for it. `CLAUDE.md` already says this about the documents in this repository; the
  README is the one place that does not keep it.
- **Copy the honesty of the Limits section.** Inillucent's names the six workloads it loses on and
  the one measurement SQLite still wins. Unluminous's equivalent is *Not included*, which is already
  written that way and only needs to stop being three screens from the end of a long file.
- **Do not copy the folder name.** Inillucent's is `docs/`; this repository's is `documentation/`,
  and it is named in 29 places across `CLAUDE.md` and fifteen task design documents, most of which
  are historical records that should not be rewritten to make a folder name match another
  repository. The layout is what the ticket asks for, and the layout is the index, the grouping and
  the writing. A rename changes none of that and breaks every one of those references.

## 3. The README that results

Around 330 lines, in this order. Every section either fits on one screen or is a table.

| Section | What is in it |
|---|---|
| Headline | Three sentences: an editor for macOS and Windows written in Rust; the desktop shows through it; everything a person can do in it an agent can do too. |
| Links | **[unluminous.com](https://unluminous.com)** first, then Install, Documentation, the command line reference, Black Rainbow Labs. |
| Facts table | Five rows, each a measured number and the page that carries it: 213 commands, 28 tools, the test count, eight crates with no user interface dependency in six of them, 20 Mermaid diagram types drawn. |
| Install | The site for the installer, and the one command a platform to build it. |
| A first window | `unluminous .`, then five `unluminous-cli` lines that do something. |
| Giving it to an AI agent | The two buttons, the MCP block, and one paragraph on why the tools are generated. |
| What it does | Eight short sections — editing, code, git, running and debugging, the terminal, the canvas, the agent panes, databases — each ending in an arrow to a page. |
| Every feature is reachable by an agent | The contract, the three mechanisms, and the second half of it that `task-1695` measured. Kept in the README rather than moved, because it is the claim the product is sold on. |
| Documentation | The index table, one row a page. |
| Limits | What is not here, shortened, with the whole list on a page. |
| Building it | `cargo build --release`, then a pointer. |
| Licence, and how to take part | Unchanged. |

The four sections that leave — Architecture, How plugins work, The command line, Tests — are 600 of
the 1,106 lines, and they leave whole rather than summarised. Nothing in them is cut. Each becomes a
page whose opening paragraph says what it covers, so a reader who followed an arrow knows they
arrived.

## 4. The documentation folder that results

Thirteen pages, of which two exist already. Each is written to be read on its own.

| | page | what it answers | where it comes from |
|---|---|---|---|
| 1 | `what-it-is.md` | what Unluminous is, who it is for, and the case for it against the editor you already use | new |
| 2 | `getting-started.md` | install it, run it, the switches, windows and projects, where the settings live | README §Installing, §Running |
| 3 | `the-window.md` | the title bar, the rail, the explorer, tabs, panes, the status bar, the seven menus, every Settings page, themes | README §What the window looks like, §The menus, §Settings |
| 4 | `editing.md` | text, formatting, Markdown, diagrams, pictures, find and replace, highlights, folding, encodings and line endings | README §What it does |
| 5 | `writing-code.md` | line numbers, tabs, syntax colouring, completion, definitions and references and rename, git, run configurations, the debugger | README §Writing code in it |
| 6 | `the-terminal.md` | the tile, the tabs, which shell, what the emulator handles, shell integration | README §What it does, and `CLAUDE.md` |
| 7 | `the-canvas.md` | the Base of Infinite Space: the six node kinds, connections, what a connection grants, how it is driven | new — nothing in the README covers it |
| 8 | `agent-panes.md` | Agent-Chat and Agent-Tasks: what each is, the five provider shapes, the permission vocabulary, the board | new |
| 9 | `for-ai-agents.md` | the contract, the three mechanisms, MCP, the two tool shapes and what each costs, reachable against reached | README §Giving it to an AI agent, §Every feature is reachable |
| 10 | `command-line.md` | the channel, the wire format, the three rules that are tests, and the MCP server on top | README §The command line |
| 11 | `architecture.md` | eight crates, inside the editor, inside the window, what one frame does, the seams, the threads, where state lives, what a frame costs | README §Architecture |
| 12 | `plugins.md` | a plugin is data, the manifest key by key, where they come from, the tokeniser, renderers, writing one | README §How plugins work |
| 13 | `testing.md` | the four layers, the numbers, the three rules the screenshot tests keep, the agent study | README §Tests |
| — | `overview.md` | what it looks like, in pictures | exists; re-shot |
| — | `database.md` | the Database plugin, in pictures | exists |
| — | `not-included.md` | everything deliberately absent, with the reason for each | README §Not included |
| — | `taking-the-pictures.md` | how the gallery is made, and how to make it again | the present `documentation/README.md`, rewritten |

`documentation/README.md` becomes the reading-order index, so the folder gains a front page and the
page that used to be its front page keeps its content under a name that says what it holds.

### 4.1 Which numbers each page owns

A number is written down once, on the page that owns it, and every other mention links there. The
README's facts table is the exception, and it is one line each with a link.

| Number | Owned by |
|---|---|
| Commands, areas, tool counts, preamble tokens | `command-line.md` |
| Test counts per crate and per layer | `testing.md` |
| Crates, frame cost, memory | `architecture.md` |
| Plugin count and what each one claims | `plugins.md` |
| Diagram types drawn and named | `editing.md` |

## 5. The pictures

### 5.1 The measurement that changes the method

`documentation/README.md` says a capture has to be a photograph of the screen, because a rendered
picture cannot show the desktop through a translucent window. The first half of that is true and the
second half is not, and it is one measurement:

**`unluminous-cli window screenshot` writes a PNG with a real alpha channel.** Started with
`--opacity 0.75` and photographed through the control channel, the editing area reads `A=191` — which
is 0.75 × 255 — with `RGB=(25,31,37)` against the `(26,31,38)` the same pixel reads at full opacity,
so the colour is **straight rather than premultiplied**. Every glyph reads `A=255`. The rounded
corners read `A=0`.

So the window's own capture already carries exactly what the compositor needs, and
`out = window.rgb × a + backdrop.rgb × (1 − a)` reproduces a screen copy byte for byte given the same
backdrop. Compositing in software rather than photographing the screen removes all four blockers at
once:

| Blocker | Why it goes |
|---|---|
| The scripts press keys with `keybd_event` | The window is driven with `unluminous-cli`, which sends no operating system input at all. `unluminous-cli input` covers the gestures that have no named command, and it feeds the window `egui::Event` down the control channel. |
| The scripts bring the window to the front | `unluminous --background` opens it without making it the foreground window, and `window screenshot` never needed the focus. |
| Every other window has to be minimised | Nothing behind the window is photographed, so nothing behind it matters. |
| It needs a 3840 by 2160 screen | The window is sized by `unluminous-cli window size`, and the capture is of the window rather than of the screen, so the screen's size stops mattering. |

It also makes the gallery **one set**. Every picture is over the same backdrop at the same size,
rather than over whatever was on that machine's desktop on the day.

### 5.2 The backdrop

A plate committed into the repository, `documentation/images/backdrop.png`, generated on this machine
through the AI service's Krea 2 endpoint — the recipe the bundled plugin icons already use and
already record. Generated rather than borrowed for one reason: a wallpaper taken off a machine is
somebody else's picture, and a gallery in a public repository cannot carry one whose licence nobody
can name.

It has to earn its place rather than be a gradient. What the gallery exists to show is that the
colour in the editing area is the desktop rather than a shade somebody chose, and that only reads
when the thing behind has real variation in it — light, colour and structure that visibly continues
underneath the window and out past its edge.

### 5.3 The harness

`tools/documentation/` — in the checkout, as `documentation/README.md` has been asking for:

| File | What it does |
|---|---|
| `capture.ps1` | starts a window with `--background` on the fixture, drives it into each state with `unluminous-cli` alone, photographs each one, and composites it onto the backdrop with a margin |
| `fixture.ps1` | builds the project every picture is taken of, under the temporary folder, with three commits of its own so the status bar says `main` rather than how many files happened to be uncommitted that day |
| `README.md` | what a re-shoot needs, and the one command that does it |

Two properties of the old scripts are kept because they are the difference between a picture of the
product and a picture of one person's machine: **the project is a fixture** built under the temporary
folder rather than this repository, and **the window is given a settings folder of its own** through
its own `APPDATA`, so the pictures carry the product's defaults rather than whatever font size and
explorer width the person running it has set, and taking them leaves nothing in their real settings.

One property is dropped: the old harness set `appearance.background.opacity = 0.830` and
`appearance.font.family = Arial`. The new one sets the opacity, because that is the setting the
gallery exists to show, and leaves everything else at the product's own defaults, because a picture
of a window nobody configured is what a reader is about to get.

### 5.4 What is photographed

Twenty-eight pictures. The twenty-two the gallery already has, re-taken; and six of the surfaces that
have never had one:

| | Picture |
|---|---|
| New | The Base of Infinite Space, with a terminal, a browser and an editor node wired together |
| New | The Agent-Chat pane mid-answer |
| New | The Agent-Tasks board |
| New | The debugger stopped on a breakpoint, with the variables and the inline values |
| New | A browser tab rendering a local page |
| New | The command palette |

### 5.5 What the page says afterwards

`overview.md` loses its whole opening apologia — the list of what has changed since 0.1.0 — because
the pictures are of the shipping version and there is nothing to apologise for. It gains the version
and the date at the top, which is what makes the next reader able to tell whether it has gone stale
again.

## 6. What this deliberately does not do

- **It does not rename `documentation/` to `docs/`.** §2 says why.
- **It does not change a line of code.** The product is not what is wrong; the description of it is.
- **It does not add a test that the documentation index matches the directory.** Inillucent has one,
  and it is the right idea. It needs a test crate that reads `documentation/` and a decision about
  where it lives, and doing it inside a documentation ticket would be a code change nobody asked for.
  It is recorded at the end of `taking-the-pictures.md` as the next thing.
- **It does not re-take `database.md`.** Those nine pictures are of `task-1777` and the plugin has not
  been redrawn since.
- **It does not touch `CLAUDE.md`.** That file is the conventions the code follows, written for
  whoever changes it next, and it is a different document with a different reader.

## 7. The order of work, and how each part is verified

| | Step | Verified by |
|---|---|---|
| 1 | Generate the backdrop | opening it |
| 2 | Write `tools/documentation/fixture.ps1` and `capture.ps1` | running them, and opening every picture |
| 3 | Re-take the gallery, and rewrite `overview.md` around the new pictures | every `![...](images/...)` resolves to a file that exists |
| 4 | Write the thirteen pages | every relative link resolves; every path named in prose exists in the checkout |
| 5 | Write `documentation/README.md` as the index | every page in the folder has a row, and every row names a page that exists |
| 6 | Rewrite `README.md` | it is under 400 lines, and every figure in it was measured rather than copied |
| 7 | Run the tests | `cargo test`, and the window suite with `--no-fail-fast` |
| 8 | Commit, push, release | `pwsh tools/release.ps1` |

A link check is a script rather than a reading: `tools/documentation/check-links.mjs` walks every
Markdown file this ticket writes, resolves every relative link and every inline path against the
checkout, and exits non-zero on the first one that is not there. That is what stops the next
re-organisation breaking a path nobody clicks.
