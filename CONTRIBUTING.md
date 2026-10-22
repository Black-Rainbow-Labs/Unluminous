# Contributing

**[`CLAUDE.md`](CLAUDE.md) is the contributor guide.** It is written for an AI agent and it is exactly
as true for a person: what each crate is for and what must never be in it, the seams, the rules that
were measured rather than chosen, and the reason beside each one. It is long because the reasons are in
it. Read the section covering the part you are changing and you will not have to undo anything.

This file adds the four things it does not say in one place.

## A feature is three things, and the second and third are not optional

Unluminous is an AI-first IDE, and that is a rule about every change rather than a slogan: **everything a
person can do in this window, an agent can do too, through the same command, and both are covered by
tests.** A change that gives a person a new control and gives an agent nothing has not been finished, in
exactly the way a change with no test has not been finished.

Most of it is automatic, and that is the point — nobody has to remember:

| what you added | what you have to do |
|---|---|
| a menu entry | nothing. `actions::menus` is walked to build `unluminous-cli action list`, and `app/action_names.rs` fails when an entry has no name |
| anything with no menu entry | a row in `unluminous-cli/src/catalogue.rs` and an arm in `app/cli.rs` |
| a command | nothing more. The MCP tools are generated from the catalogue; `every_command_is_offered_as_a_tool_in_both_shapes` fails if one is ever left out |
| a command | a section in `unluminous-cli/docs/commands.md`. `unluminous-cli/src/documentation.rs` fails while one is missing, while a usage line is stale, or while a section describes a command that has gone |

`UnluminousApp::run_cli` is the one place a command turns into a change, the way `run_action` is for the
menus, and wherever there is already a way in it uses it — so a thing done by an agent and the same thing
done by hand are the same thing. If a tool needs to say something the catalogue does not, **the catalogue
is what should say it**.

## Running the tests

There are four layers and a change should leave all four green.

```sh
cargo test -p unluminous-core -p unluminous-terminal -p unluminous-git -p unluminous-dap -p unluminous-db -p unluminous-chat
cargo test -p unluminous-app --lib
cargo test -p unluminous-app --test '*' --no-fail-fast       # the window suite: read the next paragraph
cargo run --release -p unluminous-app                        # and look at it
```

**`--no-fail-fast` is not optional on the third one.** It is fourteen separate binaries and cargo stops at
the first one that fails, so a run without that flag reports one red test while two hundred are simply
untested.

That suite builds the whole window through `egui_kittest`, renders through `wgpu` and writes a PNG per
test. **Look at the images.** They are how anybody confirms that bold text is bolder and that the
terminal's colours are right. Once accepted they are the comparison baseline, so a later change that
alters the rendering fails. `UPDATE_SNAPSHOTS=1 cargo test` accepts new ones, and **nothing should be
accepted without opening it**. Each platform has its own accepted set — macOS reads `tests/snapshots`,
Windows `tests/snapshots/windows` — because the menus, the window buttons and the font are deliberately
different there.

It needs a graphics card, so it does not run everywhere. The first two layers do.

**A performance change is measured, not asserted.** There is an example per component —
`frame_cost`, `symbol_cost`, `completion_cost`, `folding_cost`, `layout_memory`, `vello_cost`,
`startup_cost` — and `UNLUMINOUS_FRAME_TRACE=<file> unluminous` measures the whole running window. A
threshold in milliseconds would be a different number on every machine, so what a test asserts is the
*work*: how many glyphs the painter placed, how many clusters the fonts were asked to measure.

## Where a change goes

| the change is about | the crate |
|---|---|
| text, layout, undo, Markdown, the tokeniser, Mermaid | `unluminous-core` |
| the pseudoterminal, the screen, keys, the shell | `unluminous-terminal` |
| reading or changing a git repository | `unluminous-git` |
| the Debug Adapter Protocol | `unluminous-dap` |
| PostgreSQL, SQLite, Inillucent, the grid | `unluminous-db` |
| streaming from a model or an agent program | `unluminous-chat` |
| drawing, input, fonts, settings, menus, plugins | `unluminous-app` |
| the command line, the catalogue, the MCP server | `unluminous-cli` |

None of the first six may depend on a user interface, and their tests run with no window, no graphics
card and no fonts. `unluminous-cli` must not depend on `unluminous-app`; the dependency points one way, so
the client stays a small program with no window behind it.

Inside `unluminous-app`: `app/` is the window's own state, `components/` is drawing and nothing else,
`services/` is everything that is not drawing, and `theme/` is the palette, the measurements and the
drawn icons. A component takes a rectangle and returns what happened; it does not change the document.

## The house style, briefly

`design/style-guide.md` says what a control is built from and **is the thing to read before drawing
anything new** — the palette is closed, a list row is 28 points and a menu row 24, selection is one pill
drawn one way, icons are drawn rather than lettered, and every control has a plain name. Add to it
rather than inventing a second answer.

Beyond that:

- **Plain sentences.** Say what the code does and why a decision was made, once. A decision a reader
  might disagree with is recorded where it was made rather than left to be rediscovered.
- **A comment at the top of every module** saying what it is for.
- **British spelling in prose**, and the American spelling where a name in the code already uses it,
  such as `color` in `egui`.
- **Every control calls `response.widget_info` with a plain name.** The screenshot tests find controls by
  name rather than by position, and two controls must not share one.
- **A control is absent when it cannot apply, not dimmed.** Dimmed is for a control that could be used in
  a moment; absent is for one that can never apply to this file.

## Opening a pull request

Pull requests go to [`Black-Rainbow-Labs/Unluminous`](https://github.com/Black-Rainbow-Labs/Unluminous).
For anything larger than a fix, open an issue first and say what you intend, so that nobody spends a
week on a change that cannot be taken.

When you open one, GitHub fills the description in from
[`.github/pull_request_template.md`](.github/pull_request_template.md). **Every section is required.**
The template's guidance is inside `<!-- -->` comments, which are ignored, so a section you did not write
in reads as empty.

| section | what it holds |
|---|---|
| Overview | The problem or the feature, and why it matters. `Closes #N` for the issue it closes. |
| Details | How the change works and which files to read first. A Mermaid diagram where data moves between parts of the program or there is a state machine. What you weighed and rejected. |
| Agent parity | How an agent reaches what you added: the menu entry, or the catalogue row, the arm in `app/cli.rs` and the section in `docs/commands.md`. "No new control." when there is nothing a person can do. |
| Test coverage | The tests you added, by name, and what each would catch. Which of the four test layers you ran. |
| Evidence | Proof it works that a reviewer can see without building your branch. A video, preferably, or screenshots before and after, for anything visible; the command and its output otherwise; numbers and the command that produced them for speed or memory. |
| Platforms | Every platform you built and ran it on. |
| AI disclosure | Which agent and model you used and for what, or "None". |
| Checklist | Every box ticked. |

### What the standard for tests is

- **A change to Rust code adds a test.** If it genuinely cannot have one, a line under `Test coverage`
  starting `No new test:` says why. A rename of a private function is a reason; "it is a small change" is
  not.
- **Behaviour is tested where it lives.** The six crates with no user interface have unit tests that run
  with no window. A test in `unluminous-app` is for what only the window can show.
- **A change to what the window draws has a screenshot test**, and its accepted picture is in the pull
  request. Say that you opened it.
- **A new command is reached from a test the way a person reaches it**, through `run_cli` or
  `run_action`, rather than by calling the function underneath.
- **A test asserts the work, not the clock.** See "A performance change is measured, not asserted" above.

### What is checked, and by what

Three checks run on every pull request, and a pull request is not landed while any of them fails:

- **PR description** reads the description against the template and keeps one comment on the pull
  request saying exactly what is missing. Edit the description and it runs again. It also asks for a
  video or screenshots when the change touches `components/`, `theme/`, `design/`, the application icon
  or an accepted screenshot, and for a test or a `No new test:` line when the change touches Rust code.
  It runs from `main`, so a pull request cannot change the check that judges it.
- **Format** runs `cargo fmt --all -- --check` with the toolchain `rust-toolchain.toml` names.
- **Headless tests** runs the tests of `unluminous-core`, `unluminous-terminal`, `unluminous-git`,
  `unluminous-dap` and `unluminous-chat` on Windows.

Clippy and the window suite need the whole workspace and a graphics card, so they are run on the
maintainer's machine before a release rather than here. Run them yourself before you open the pull
request; the checklist asks you to.

### How a pull request is landed

**A pull request is landed rather than merged, so it will show as closed rather than merged.** The public
repository is published from a private one where releases are cut, by a push that has to be a fast
forward, and a merge made on GitHub would put a commit on the public `main` that the private repository
does not have. So a maintainer applies your change there with `tools/land-pull-request.mjs`, which
commits it **with you as the author** and every other author of your commits as a co-author, releases
it, and closes the pull request with a comment naming the commit it became. The commit carries a
`Landed-from: Black-Rainbow-Labs/Unluminous#N` trailer, which is what keeps your name on it when the
history is published.

Keep your branch to one change. It is landed as one commit, so the pull request's title becomes the
commit's subject and its `Overview` becomes the commit's body: write both so that they still make sense
in `git log` a year from now.
