# For AI agents

Unluminous is built to be driven by a person and by an AI agent equally, and the rule is one sentence:

> **Every piece of functionality a person can reach, an agent can reach, through the same command,
> and both are covered by automated tests.**

That is a contract about *new work* as much as about what is here. A feature that ships with a menu
entry and no way for an agent to ask for it is an unfinished feature, in the same way a feature with
no test is an unfinished feature. There is no lane where one of the two is optional.

## Installing it into an agent

`Settings -> Tools -> MCP`, then **Install for Claude Code** or **Install for Codex**. Restart the
agent and it can drive Unluminous: open files, read and change the text, run things in the terminal,
search the project, work the Git menu, drive the debugger, and take a screenshot of the real window
and look at it.

For anything else that speaks the Model Context Protocol, the block to paste is on the same page:

```json
{
  "mcpServers": {
    "unluminous": {
      "command": "unluminous-cli",
      "args": ["mcp", "serve"]
    }
  }
}
```

The installers use the agent's own command first — `claude mcp add-json` and `codex mcp add` — because
`~/.claude.json` is rewritten by every running Claude Code and is a hundred kilobytes of somebody's
settings, and Codex's `config.toml` is hand-written and holds comments. The direct edit is the
fallback, it takes a copy first, and it changes one key or one table and nothing else.

`unluminous-cli/docs/mcp.md` is the whole of it.

## The three mechanisms that make the rule true

None of them is a promise anybody has to remember to keep.

**A menu entry needs nothing at all.** `unluminous-cli action list` is built by walking the real menus,
so an entry added tomorrow can be run from the command line and by an agent tomorrow. A test fails the
day a menu entry has no name.

**Anything with no menu entry is a row in the catalogue** — one list, in a crate both the client and
the window depend on, so a command the client accepts is a command the window knows. `run_cli` is to
that list what `run_action` is to the menus: the single place a command turns into a change, using the
same path a person's click takes.

**The MCP tools are generated from that catalogue.** A command added to Unluminous is a tool the day
it is added, with its summary, its arguments and its flags, and a test fails if one ever is not. A
hand-written set of tools would be a third copy of what Unluminous can do, which is the exact thing
the catalogue exists to prevent.

And the documentation is a test too: one fails while a command has no section in
`unluminous-cli/docs/commands.md`, while a usage line is out of date, or while a section describes a
command that no longer exists. A second parses every example in the catalogue and checks it runs the
command it is filed under, because the examples are what an agent copies.

**One command is held back**, and the list of exclusions is itself a test: `mcp serve` would start a
server from inside one. A test fails if that list ever grows, because an exclusion nobody argued about
is how "everything is reachable" quietly stops being true.

## What an agent is given, and what it costs

**One tool an area is the default, and it was measured rather than assumed.**

| | tools | tokens |
|---|---:|---:|
| `mcp.tools = grouped`, every area | 28 | **27,011** |
| `mcp.tools = every`, every area | 213 | 58,579 |
| `--areas editor,git`, grouped | 7 | **6,218** |
| `--areas editor`, grouped | 6 | 5,439 |

213 commands would be 213 tool definitions in an agent's context on every conversation, for more than
twice what 28 area tools cost — and those 28 still carry every command's usage line and summary.
`unluminous-cli mcp tools --count` prints both figures against the catalogue as it is now, so the
choice is never made against a number in a comment. `mcp.tools = every` is there for a client that
permits tools by name and would rather pay.

**`mcp serve --areas editor,git` is how an agent is equipped with less than all of it.** The commands
with no area — `status`, `launch`, `quit` — are always offered, because an agent that cannot ask
`status` cannot use any of the others.

Two properties are on every tool in both shapes, and both are about the *call* rather than the
command: `instance`, which window to drive, and `timeout`, how long to wait. Both are generated in one
place, so a tool added tomorrow has them.

### An area tool's schema is a union, and two things follow

`unluminous_editor` and its siblings take `{ "command": <verb>, "arguments": { … } }`, and the nested
object is **every argument and flag of every command in the area**. That is what keeps the preamble
small enough to hand to a model at all, and a study run measured what it costs when nobody thinks
about it.

- **A key the schema offered is dropped, not refused.** A model sent `plugins run` with `pane`,
  `show`, `side` and `tab` — all real keys of *sibling* verbs, all in the schema — and the window
  refused it, because a key a command does not name is a usage refusal. It tried the same call **68
  times** and the whole `database` area went unused. Sibling keys are taken out and **named in the
  answer** now; a key that is nothing at all still reaches the window and is still refused, so a typo
  is still a typo.
- **A command whose own argument shares a name with the tool's is renamed inside the call.**
  `plugins run` takes `command` and `arguments`, which are exactly the tool's own two keys, so the
  model could not say both. Those are offered as `verb` and `words` and mapped back; the command line
  and the wire are unchanged.

## Reachable is not the same as reached

That machinery guarantees an agent *can* do everything. It does not guarantee an agent *will*, and
that difference was measured by watching a local Qwen 3.8 27B drive a real window across 23 scenarios
phrased the way a person speaks.

It made 126 tool calls, and **30 of them — 24%, in 13 of the 23 scenarios — went to its own `grep`,
`bash`, `read` and `edit`** for jobs Unluminous has a first-class command for: git status,
find-references, go-to-definition, making a folder, and renaming a symbol across the project.

The rename is the one that shows why it matters. `editor rename` is one undo step per file, it leaves
comments and strings alone unless asked, and it knows which files are open. The agent's replace-all
was none of those things, and it silently rewrote a Mermaid diagram that the rename would have left
alone. And in the debug scenario it drove the debugger correctly and then answered the value of a
variable **by doing arithmetic on the source**, because the value it had already been handed was
buried behind nineteen stack frames.

So the contract has a second half, and it is the harder one: **a feature is not finished when an agent
*can* use it, but when an agent *does*.** Four things decide that, and each has a ticket behind it:

- **Name it the way an agent guesses.** `editor open` was tried three times; it is `tab open`.
  Argument names are kebab-case and a model writes camelCase, so both are accepted.
- **Answer in a payload proportionate to the question.** An agent handed 3,000 tokens to learn one
  number stops asking.
- **Say what Unluminous knows that a file tool does not** — that `editor references` classifies a hit
  as code, comment or string; that `git` runs the machine's real git with its credential helper. If
  the description does not say it, `grep` wins.
- **Never make a read-only question mutate the document.** `editor complete --stem ar` answers what a
  hypothetical word would offer without typing into the person's file.

A rerun of the five misses found one more selection rule: **a broad area tool does not reliably
compete with a dedicated generic tool merely because its long description is better.** Put the exact
intent in the title, and for semantic definition, references and rename keep the additive narrow
tools. They do not replace the area tool or duplicate catalogue data; they give the chooser an equally
specific Unluminous answer. The final pass used Unluminous for the primary operation in all five
scenarios with no refusals.

## How to check it rather than assume it

`tools/agent-study/` drives a real window through a local model and grades what happened against
Unluminous's own state read back through `unluminous-cli`, rather than against what the agent said it
did. **Add a scenario when you add a feature.** A feature nobody has watched an agent use is a feature
nobody knows is reachable in practice.

`unluminous-cli/agent-assessment/qwen-38-27B-assessment.md` is the other measurement: the same local
model, given only `docs/commands.md`, carries out 64 instructions phrased as a person would say them
and scores **100%**, five rounds running, at two temperatures. The same 64 instructions with the
documentation withheld score **3.13%**, which is what makes the first number mean something.

## Driving the window without taking the keyboard

**Never activate an Unluminous window to drive it.** Synthetic operating system input goes to whatever
window is in *front*, which is what forces a script to bring one forward — and on Windows activating a
window that is on another virtual desktop switches the desktop with it, which is a machine somebody
else is using being taken away from them.

Three things make it unnecessary, and each is a thing rather than a habit:

- **`unluminous --background`** opens a window without making it the foreground window.
- **`unluminous-cli input`** clicks, types, drags and scrolls by feeding the window the same events a
  mouse and a keyboard produce, down the control channel. Positions are the window's own points, which
  is what `window screenshot` writes out, so a position measured off a picture is the position to
  send.
- **`unluminous-cli window screenshot`** photographs the window whether or not it is in front, whether
  or not it is covered, and whether or not the desktop being looked at is the one it is on. It never
  needed the focus, and it never did.

`tools/drive-a-window.ps1` and `tools/drive-a-window.sh` start a window the right way on each
platform. **`input` is the last resort, not the first**: a command that names the thing — `action run`,
`tab open`, `space editor` — reaches the same code and does not depend on where anything was drawn.
`input` is for the gestures there is no other command for.

[Taking the pictures](taking-the-pictures.md) is the whole documentation gallery driven that way,
which is the working example.

## A front door of its own

`AGENTS.md` in the repository root is written for an agent rather than for a person, and it splits the
two audiences: using Unluminous, and changing it. `GEMINI.md` points at it. `CLAUDE.md` is the
conventions the code already follows, for whoever changes it next.
