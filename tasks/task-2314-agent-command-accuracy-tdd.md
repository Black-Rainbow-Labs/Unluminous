# task-2314: agents that find the right Unluminous command the first time

## The ask

> Agents tend to spend a good deal of time looking through unluminous commands. I have the mcp set up.
> Use our opencode qwen for testing and verification. Use build eval and hillclimb skills to drastically
> improve accuracy. Can skills be added during install? Can our descriptions be better in our tooling?
> Can batch operations help? Add documentation that captures agent accuracy metrics, so we can monitor
> it over time.

## What is measured, and how

**The eval is `tools/agent-study/eval.mjs` over `tools/agent-study/cases.mjs`.** The cases are the 37
agent study scenarios plus 17 added for this ticket, aimed at commands an agent has to find rather than
guess: go to a line, close every tab but one, dock a panel, size the explorer, duplicate a line, open a
folder's files, highlight several lines, change the indent, show source and preview side by side,
reopen a closed tab, change the keymap, move a file, search for a word, hide the line numbers, run a
command and read a number out of it, count a file's lines, and edit and save two files. 52 are graded.
`s34-members-and-signature` needs rust-analyzer, which `editor.servers` leaves off in a fresh settings
folder, and `s36-dictation` needs the speech model; both are left out because no description can change
them.

**The agent is opencode 1.18.32 on the local Qwen 3.8 27B** served on `:8080`, with every other MCP
server in the person's own opencode configuration switched off. A hosted model would spend the
account's usage on every round.

**Each case is one fresh window** (`tools/agent-study/window.mjs`): a new copy of the sample project, a
settings folder copied from a template, and a window started with `--background` and its own
`APPDATA`. The agent's MCP server is given `UNLUMINOUS_INSTANCES` and `UNLUMINOUS_INSTANCE`, so it can
only find that window. A theme, a font size, a ticket or a canvas cannot carry from one case into the
next, and the person's own windows and settings are never touched.

**Grading reads the window's state back** through `unluminous-cli` and the disk. Where the job is to
tell the person something, the answer is checked for the facts only the project gives (the total area
is 35.5, the biggest table has 7 rows). The grades on every case:

| Grade | Meaning |
|---|---|
| `success` | The job was done. The headline. |
| `clean` | Done, with no refused call and no call to the agent's own tools. |
| `refused` | Calls the server refused or answered as an error. |
| `discovery` | Calls that only asked what commands there are. |
| `own` | Calls to the agent's own `bash`, `read`, `edit`, `grep` and the rest. |
| `calls` | Every tool call the case took. |

The split is fixed by name: a case is in the test split when the FNV-1a hash of its id is 0 mod 3.

## What the baseline showed

Measured on the shipped 1.3.2 build, 52 cases, 2 reps: success 84%, but only 48% of cases were clean.
Each case averaged 0.72 refused calls and 0.98 calls to the agent's own tools. Success was close to its
ceiling, so the climb aimed at `clean` and held `success`.

An analyzer read every failing train case (`_agent_output/task-2314/flow/v1/analysis-baseline.md`) and
a Fable advice ticket read the whole baseline (`task-2315`, `_agent_output/task-2314-fable-advice/`).
Both found that the largest causes were not the model's:

- **`realm chat` could not be called at all.** The grouped tools rename a command's own `command` and
  `arguments` arguments to `verb` and `words`, so they do not collide with the tool's own keys. The
  rename was applied to every command, and `realm chat`'s own arguments are called `verb` and `words`,
  so its `verb` was turned into `command` and refused. 21 of the 39 refusals in the train split were
  this, in one case.
- **A renamed key sent beside `command` was dropped** rather than read, so `plugins run` with
  `verb: "add-source"` was refused with "Say which command".
- **`tab open` replaced the tab before it.** It reused the explorer's single click tab, so opening two
  files left one, and the model reported one file open, or split the view to keep both.
- **Plugin verbs were nowhere in a description.** `plugins run <id> <command>` takes a free word that
  only `plugins show` lists.
- **Editor commands acted only on the tab that was showing**, so a job naming a file was two calls, and
  the model skipped both and edited the file on disk.
- **Bugs:** `add-source` kept a relative path that `connect` then could not find; `terminal send` turned
  `\t` in a Windows path into a tab; there was no way to get a diff as text; a quoted phrase broke a
  plain English search; an unknown setting key named nothing nearer.

## What was changed

Each round is one hypothesis, measured on all 52 cases at 2 reps on top of the round before.

| Round | Change |
|---|---|
| v1 | The server accepts the call the agent meant: the rename only for a command that has those argument names; renamed keys read beside `command`; a missing positional read from the first word of the rest of the line, or from the one key the command does not have; `tab open` opens a tab of its own (`--transient` keeps the old behaviour); `settings list <words>` filters and an unknown setting names the nearest keys; `add-source` resolves a relative file against the project. |
| v2 | Every area description has a `Call:` line under each command, its first catalogue example written as the JSON the tool takes, and every refusal ends with that call. |
| v3 | A tool for each plugin that draws, generated from the plugin's own command table, which moved into `unluminous-cli/src/plugins.rs` so the window and the tools read one list. Editor and fold commands and `tab save` take `--path`. `git diff`. `terminal send` sends text as written. Quoted phrases in a plain English search. |
| v4 | The server instructions and the first sentence of the `tab`, `explorer` and `editor` descriptions say which tool replaces `read`, `grep`, `glob`, `ls` and `edit`. |
| v5 | v4 with each tool named by its area ("the search tool") in the instructions and the skill. |
| v6 | `unluminous_batch`: several command lines in one call. |
| v7 | The Unluminous skill, placed where opencode reads a project's skills. |
| v8 | Three server faults found in the v6 refusals: only an object is sent as `structuredContent`; a narrow tool reads keys wrapped in `arguments`; the tool with no area, given an area's name as `command`, is sent to that area's tool. |

## What it found

| Round | Success, test | Success, training | Clean | First call right | Refused a case | Own reads a case | Own writes a case | Kept |
|---|---|---|---|---|---|---|---|---|
| 1.3.2 | 87% ±12 | 82% ±9 | 48% ±10 | 91% | 0.72 | 0.71 | 0.27 | |
| v1 | 80% ±14 | 86% ±8 | 54% ±10 | 96% | 0.19 | 0.65 | 0.20 | yes |
| v2 | 77% ±15 | 86% ±8 | 57% ±10 | 96% | 0.29 | 0.63 | 0.15 | no |
| v3 | 90% ±11 | 92% ±6 | 55% ±10 | 89% | 0.30 | 0.66 | 0.05 | yes |
| v4 | 77% ±15 | 85% ±8 | 67% ±9 | 98% | 0.13 | 0.50 | 0.14 | changed in v5 |
| v5 | 87% ±12 | 89% ±7 | 66% ±9 | 95% | 0.23 | 0.46 | 0.22 | yes |
| v6 | 83% ±13 | 92% ±6 | 64% ±9 | 95% | 0.36 | 0.37 | 0.05 | yes |
| v7 | 93% ±9 | 93% ±6 | 66% ±9 | 92% | 0.36 | 0.26 | 0.11 | yes |
| v8 | 83% ±13 | 92% ±6 | 64% ±9 | 96% | 0.26 | 0.25 | 0.06 | yes |

v8 removed the refusals it was aimed at (0.36 a case to 0.26) and halved the agent's own writes. Its
held out success reads lower than v7's by less than the interval, and the four cases that lost a rep
did not reach any of the three fixes, so v8 is kept and the shipped build is v8.

The ticket's three questions, answered from these rows:

- **Can skills be added during install?** Yes. `mcp install` and `setup` write `skills/unluminous/SKILL.md` into `~/.claude/skills` and `~/.agents/skills` (`mcp::skill`), and a test fails if the skill names a tool or a verb that does not exist. v7 measured it: success on the held out cases rose from 83% to 93%, and the agent's own reads fell from 0.37 a case to 0.26.
- **Can the descriptions be better?** Partly. Naming the tool that replaces each of the agent's own tools (v4, v5) took clean from 55% to 66%. Putting an example under every command (v2) bought nothing and cost 3,900 tokens a request. What helped most was the server reading a call the way it was meant (v1, v8): most refusals in the baseline were the server's fault, not the model's.
- **Can batch operations help?** Yes, for writes: with `unluminous_batch` the agent's own writes fell from 0.22 a case to 0.05, and training success rose from 89% to 92%. Test success moved from 87% to 83%, inside the noise.

`documentation/agent-accuracy.md` keeps the table and says how to run it again.
